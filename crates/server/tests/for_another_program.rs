//! What an estimating system needs from the server, run for real over a
//! socket: the same bid is always the same project, the same drawing is never
//! stored twice, a drawing issued again brings its takeoff with it, a finished
//! bid is put away rather than destroyed, and a pull says exactly how many of
//! everything there are.

use std::sync::Arc;

use hub::Client;
use hyperview_server::api::{self, Server};
use hyperview_server::{auth, Config, Store};
use rusqlite::params;
use serde_json::{json, Value};

struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new() -> Scratch {
        let n: u64 = rand::random();
        let path = std::env::temp_dir().join(format!("hyperview-program-{n:016x}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Running {
    base: String,
    token: String,
    scratch: Scratch,
    _runtime: tokio::runtime::Runtime,
}

fn start() -> Running {
    let scratch = Scratch::new();
    let config = Config {
        name: "Mesa Fab".into(),
        data: scratch.0.clone(),
        listen: "127.0.0.1:0".parse().unwrap(),
        ..Config::default()
    };
    let store = Store::open(&config.database(), &config.blobs()).expect("a store");
    let hashed = auth::hash_password("brace-gusset-purlin-shim-42").unwrap();
    store
        .with(|db| {
            db.execute(
                "INSERT INTO people (id, name, email, password, role, created)
                 VALUES (?1, 'Creede', 'creede@mesafab.com', ?2, 'admin', ?3)",
                params![api::fresh_id("usr"), hashed, api::now()],
            )?;
            Ok(())
        })
        .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("a runtime");
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind(config.listen))
        .expect("a socket");
    let address = listener.local_addr().expect("an address");
    let app = api::router(Arc::new(Server::new(store, config)));
    runtime.spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let base = format!("http://{address}");
    let mut client = Client::new(&base);
    client.sign_in("creede@mesafab.com", "brace-gusset-purlin-shim-42").expect("sign in");
    Running {
        base,
        token: client.token().unwrap().to_string(),
        scratch,
        _runtime: runtime,
    }
}

impl Running {
    fn get(&self, path: &str) -> Value {
        ureq::get(&format!("{}/api/v1{path}", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .call()
            .unwrap_or_else(|e| panic!("GET {path}: {e}"))
            .into_json()
            .unwrap()
    }

    fn post(&self, path: &str, body: Value) -> Value {
        ureq::post(&format!("{}/api/v1{path}", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(body)
            .unwrap_or_else(|e| panic!("POST {path}: {e}"))
            .into_json()
            .unwrap()
    }

    fn upload(&self, project: &str, name: &str, bytes: &[u8], carry: bool) -> hub::DrawingSet {
        let boundary = "----hyperviewprogram";
        let mut body: Vec<u8> = Vec::new();
        let mut field = |k: &str, v: &str| {
            body.extend_from_slice(
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                    .as_bytes(),
            );
        };
        field("project", project);
        field("name", name);
        if carry {
            field("carry_forward", "1");
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; \
                 filename=\"{name}\"\r\nContent-Type: application/pdf\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        ureq::post(&format!("{}/api/v1/sets", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body)
            .expect("upload")
            .into_json()
            .expect("a drawing set")
    }

    fn push(&self, set: &str, markups: &[String]) {
        let list: Vec<Value> = markups
            .iter()
            .map(|d| json!({ "page": 0, "dictionary": d }))
            .collect();
        self.post(&format!("/sets/{set}/markups"), Value::Array(list));
    }

    /// What Claude would be told: one tool call on the server's MCP.
    fn mcp(&self, tool: &str, arguments: Value) -> String {
        let said: Value = ureq::post(&format!("{}/mcp", self.base))
            .set("Authorization", &format!("Bearer {}", self.token))
            .send_json(json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                               "params": { "name": tool, "arguments": arguments } }))
            .unwrap_or_else(|e| panic!("MCP {tool}: {e}"))
            .into_json()
            .unwrap();
        said.pointer("/result/content/0/text").and_then(|t| t.as_str()).unwrap_or_default().to_string()
    }

    fn set_scale(&self, set: &str) {
        let db = rusqlite::Connection::open(self.scratch.0.join("hyperview.sqlite")).unwrap();
        db.execute(
            "UPDATE sheets SET scale = '1/4\" = 1''-0\"' WHERE set_id = ?1",
            params![set],
        )
        .unwrap();
    }
}

/// A one-page drawing of the given size. `mark` changes the bytes without
/// changing the drawing, the way a re-issue does.
fn a_drawing(width: u32, height: u32, mark: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("%PDF-1.7\n%{mark}\n").as_bytes());
    let mut offsets = Vec::new();
    for body in [
        "1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n".to_string(),
        "2 0 obj\n<</Type/Pages/Kids[3 0 R]/Count 1>>\nendobj\n".to_string(),
        format!("3 0 obj\n<</Type/Page/Parent 2 0 R/MediaBox[0 0 {width} {height}]>>\nendobj\n"),
    ] {
        offsets.push(out.len());
        out.extend_from_slice(body.as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1)
            .as_bytes(),
    );
    out
}

fn encoded(m: annot::Markup) -> String {
    use base64::Engine;
    let mut bytes = Vec::new();
    pdf::write::write_object(&pdf::Object::Dict(m.dict), &mut bytes);
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A W12x26, `points` long, standing for `quantity` beams.
fn a_beam(points: f64, quantity: f64) -> String {
    let mut m = annot::Markup::new(annot::Subtype::Line);
    m.set("IT", pdf::Object::name("LineDimension"));
    m.set_subject("W12x26");
    m.set_line([100.0, 100.0], [100.0 + points, 100.0]);
    if quantity != 1.0 {
        m.set("HVQuantity", pdf::Object::real(quantity));
    }
    encoded(m)
}

/// A moment-connection count of `clicks`.
fn moments(clicks: usize) -> String {
    let mut m = annot::Markup::new(annot::Subtype::Polygon);
    m.set("IT", pdf::Object::name("PolygonCount"));
    m.set_subject("Moment Conn");
    let points: Vec<[f64; 2]> = (0..clicks).map(|i| [200.0 + 20.0 * i as f64, 300.0]).collect();
    m.set_vertices(&points);
    m.set("NumCounts", pdf::Object::Int(clicks as i64));
    encoded(m)
}

fn every_markup(pull: &Value) -> Vec<Value> {
    pull["results"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|r| r["markups"]["Markups"].as_array().cloned().unwrap_or_default())
        .collect()
}

#[test]
fn the_same_bid_is_always_the_same_project_and_putting_it_away_destroys_nothing() {
    let server = start();
    let first = server.post(
        "/projects",
        json!({ "number": "2630", "name": "Riverside Medical", "reference": "fabwire:bid:2630" }),
    );
    let again = server.post(
        "/projects",
        json!({ "number": "2630", "name": "Riverside Medical Center", "reference": "fabwire:bid:2630" }),
    );
    assert_eq!(first["id"], again["id"], "a retried push must not make a second job");
    assert_eq!(again["name"], "Riverside Medical Center", "a renamed bid renames its project");
    let id = first["id"].as_str().unwrap().to_string();
    let set = server.upload(&id, "S-101", &a_drawing(3024, 2160, "a"), false);
    server.push(&set.id, &[a_beam(720.0, 1.0)]);

    let put_away = server.post(&format!("/projects/{id}/archive"), json!({}));
    assert!(put_away["archived"].is_string());
    let listed = server.get("/projects");
    assert!(
        listed.as_array().unwrap().iter().all(|p| p["id"] != id.as_str()),
        "an archived job is off the list"
    );
    let everything = server.get("/projects?all=1");
    assert!(everything.as_array().unwrap().iter().any(|p| p["id"] == id.as_str()));
    // Nothing on it changed.
    let pull = server.get(&format!("/projects/{id}/markuplist"));
    assert_eq!(every_markup(&pull).len(), 1, "archiving is not deleting");
    assert!(pull["archived"].is_string());

    // Pushing to it again brings it back.
    let back = server.post(
        "/projects",
        json!({ "number": "2630", "name": "", "reference": "fabwire:bid:2630" }),
    );
    assert_eq!(back["id"], id.as_str());
    assert!(back.get("archived").is_none() || back["archived"].is_null());
    assert_eq!(back["name"], "Riverside Medical Center", "a blank name keeps the one it had");
}

#[test]
fn the_same_drawing_twice_is_one_drawing() {
    let server = start();
    let project = server.post("/projects", json!({ "number": "2631", "name": "Shop" }));
    let id = project["id"].as_str().unwrap();
    let bytes = a_drawing(3024, 2160, "same");
    let first = server.upload(id, "S-101 Framing.pdf", &bytes, true);
    assert!(!first.already_here);
    let second = server.upload(id, "S-101 Framing.pdf", &bytes, true);
    assert!(second.already_here, "the second push of the same bytes is the first set");
    assert_eq!(second.id, first.id);
    let sets = server.get(&format!("/projects/{id}/sets"));
    assert_eq!(sets.as_array().unwrap().len(), 1);
}

#[test]
fn a_drawing_issued_again_brings_its_takeoff_and_says_it_needs_a_look() {
    let server = start();
    let project = server.post("/projects", json!({ "number": "2632", "name": "Addendum" }));
    let id = project["id"].as_str().unwrap();
    let old = server.upload(id, "S-201.pdf", &a_drawing(3024, 2160, "rev0"), true);
    server.set_scale(&old.id);
    // Two forty-foot beams drawn as one markup standing for two, and a count
    // of seven moment connections.
    server.push(&old.id, &[a_beam(720.0, 2.0), moments(7)]);
    let before = server.get(&format!("/projects/{id}/stamp"));

    let new = server.upload(id, "s-201", &a_drawing(3024, 2160, "rev1"), true);
    assert_eq!(new.supersedes.as_deref(), Some(old.id.as_str()));
    assert_eq!(new.carried_forward, 2, "{:?}", new.carry_note);
    assert!(new.carry_note.is_none());
    server.set_scale(&new.id);

    let after = server.get(&format!("/projects/{id}/stamp"));
    assert_ne!(before["stamp"], after["stamp"], "a re-issue changes the stamp");
    assert_eq!(after["superseded"], 1);
    assert_eq!(after["carried_forward"], 2);

    let pull = server.get(&format!("/projects/{id}/markuplist"));
    assert_eq!(pull["results"].as_array().unwrap().len(), 1, "only the issue that counts");
    assert_eq!(pull["superseded"][0]["set"], old.id.as_str());
    assert_eq!(pull["superseded"][0]["markups"], 2, "and the old one says what it still holds");
    let markups = every_markup(&pull);
    let beam = markups.iter().find(|m| m["Subject"] == "W12x26").expect("the beam");
    let count = markups.iter().find(|m| m["Subject"] == "Moment Conn").expect("the count");
    assert_eq!(beam["Hyperview"]["Each"], 2.0, "two beams");
    assert!((beam["Hyperview"]["LengthFt"].as_f64().unwrap() - 40.0).abs() < 1e-6);
    assert_eq!(count["Hyperview"]["Each"], 7.0, "seven connections, exactly");
    assert_eq!(count["Hyperview"]["Kind"], "Count");
    for m in &markups {
        assert_eq!(m["Hyperview"]["CarriedForward"], true);
        assert_eq!(m["Hyperview"]["NeedsCheck"], true);
        assert_eq!(m["Status"], "Carried forward — check");
    }
    // The old issue still has its own, untouched.
    let everything = server.get(&format!("/projects/{id}/markuplist?all=1"));
    assert_eq!(everything["results"].as_array().unwrap().len(), 2);

    // And Claude, asking the office, is told which issue counts and what
    // still needs a look — by the job number, which is what people say.
    let sets = server.mcp("list_sets", json!({ "project": "2632" }));
    assert!(sets.contains("2 drawing set(s), 1 current"), "{sets}");
    assert!(sets.contains("NOT the job's quantities"), "{sets}");
    assert!(sets.contains(&format!("superseded by {} [id {}]", new.name, new.id)), "{sets}");
    assert!(sets.contains("2 markup(s) carried forward"), "{sets}");
    let current = sets.find(&new.id).unwrap();
    let old_at = sets.find(&format!("[id {}]", old.id)).unwrap();
    assert!(current < old_at, "the issue that counts comes first: {sets}");
    let jobs = server.mcp("list_projects", json!({}));
    assert!(jobs.contains("2632  Addendum  —  1 current drawing set(s)"), "{jobs}");
}

#[test]
fn a_takeoff_is_never_carried_onto_a_sheet_that_changed() {
    let server = start();
    let project = server.post("/projects", json!({ "number": "2633", "name": "Resized" }));
    let id = project["id"].as_str().unwrap();
    let old = server.upload(id, "S-301", &a_drawing(3024, 2160, "rev0"), true);
    server.push(&old.id, &[moments(3)]);
    let new = server.upload(id, "S-301", &a_drawing(2592, 1728, "rev1"), true);
    assert_eq!(new.carried_forward, 0);
    let note = new.carry_note.expect("it says why");
    assert!(note.contains("changed size"), "{note}");

    // Asked not to carry, nothing is carried and nothing is said.
    let project = server.post("/projects", json!({ "number": "2634", "name": "Plain" }));
    let id = project["id"].as_str().unwrap();
    let old = server.upload(id, "S-401", &a_drawing(3024, 2160, "rev0"), false);
    server.push(&old.id, &[moments(3)]);
    let new = server.upload(id, "S-401", &a_drawing(3024, 2160, "rev1"), false);
    assert_eq!(new.supersedes.as_deref(), Some(old.id.as_str()));
    assert_eq!(new.carried_forward, 0);
    assert!(new.carry_note.is_none());
}

#[test]
fn the_stamp_moves_when_a_markup_does_and_not_otherwise() {
    let server = start();
    let project = server.post("/projects", json!({ "number": "2635", "name": "Stamp" }));
    let id = project["id"].as_str().unwrap();
    let set = server.upload(id, "S-501", &a_drawing(3024, 2160, "x"), false);
    let a = server.get(&format!("/projects/{id}/stamp"));
    let b = server.get(&format!("/projects/{id}/stamp"));
    assert_eq!(a["stamp"], b["stamp"], "reading changes nothing");
    server.push(&set.id, &[moments(1)]);
    let c = server.get(&format!("/projects/{id}/stamp"));
    assert_ne!(a["stamp"], c["stamp"]);
    assert_eq!(c["markups"], 1);
}

#[test]
fn an_office_whose_trial_ended_keeps_everything_readable_and_only_sharing_pauses() {
    let server = start();
    let project = server.post("/projects", json!({ "number": "2640", "name": "Trial" }));
    let id = project["id"].as_str().unwrap().to_string();
    let set = server.upload(&id, "S-101.pdf", &a_drawing(3024, 2160, "t"), false);
    server.push(&set.id, &[moments(3)]);

    // A server set up before licensing is a founding install.
    let standing = server.get("/license");
    assert_eq!(standing["state"], "founding", "{standing}");
    assert_eq!(standing["sharing"], true);

    // Make this one an ordinary office whose trial ended long ago.
    {
        let db = rusqlite::Connection::open(server.scratch.0.join("hyperview.sqlite")).unwrap();
        db.execute("DELETE FROM settings WHERE key = 'founding_install'", []).unwrap();
        db.execute(
            "UPDATE settings SET value = '2026-01-01T00:00:00Z' WHERE key = 'licensing_began'",
            [],
        )
        .unwrap();
    }
    let standing = server.get("/license");
    assert_eq!(standing["state"], "unlicensed", "{standing}");
    assert_eq!(standing["sharing"], false);
    assert!(standing["message"].as_str().unwrap().contains("Nothing is deleted"));

    // Everything already there still reads and exports.
    let pull = server.get(&format!("/projects/{id}/markuplist"));
    assert_eq!(every_markup(&pull).len(), 1);
    let csv = ureq::get(&format!("{}/api/v1/sets/{}/takeoff.csv", server.base, set.id))
        .set("Authorization", &format!("Bearer {}", server.token))
        .call()
        .expect("the takeoff still exports");
    assert_eq!(csv.status(), 200);

    // Sharing new work is refused in words, never with a sign-out.
    let refused = ureq::post(&format!("{}/api/v1/projects", server.base))
        .set("Authorization", &format!("Bearer {}", server.token))
        .send_json(json!({ "number": "2641", "name": "New" }));
    match refused {
        Err(ureq::Error::Status(403, response)) => {
            let body: Value = response.into_json().unwrap();
            assert_eq!(body["error"], "license_needed");
            assert!(body["message"].as_str().unwrap().contains("paused"));
        }
        other => panic!("expected 403 license_needed, got {other:?}"),
    }
    // Nobody is signed out: the same token still reads.
    assert_eq!(server.get("/me")["email"], "creede@mesafab.com");

    // A license that nobody signed changes nothing.
    let forged = ureq::post(&format!("{}/api/v1/license", server.base))
        .set("Authorization", &format!("Bearer {}", server.token))
        .send_json(json!({ "license": "{\"id\":\"evl_1\",\"company\":\"Me\",\"edition\":\"office\",\"users\":0,\"updates_through\":\"forever\",\"issued\":\"2026-11-02\",\"key\":\"mesafab-2026\",\"signature\":\"00\"}" }));
    assert!(matches!(forged, Err(ureq::Error::Status(400, _))), "{forged:?}");
    assert_eq!(server.get("/license")["state"], "unlicensed");
}
