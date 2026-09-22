//! Telling Excalibur Fleet about a company's server, from the computer Fleet
//! runs on.
//!
//! Fleet is the publisher's own board of every install in the field. It lives
//! in the tray on the publisher's computer and listens on a named pipe. When an
//! administrator presses "Let Excalibur Fleet watch this server" on that same
//! computer, the server makes a maintenance key, and this hands the address and
//! the key straight to Fleet — so the key is never shown, copied or pasted
//! anywhere. On any other computer there is no Fleet to hand it to, and the
//! program shows the key once for whoever looks after that company's machines.

/// Fleet's door. The same name its own MCP relay uses.
pub const PIPE: &str = r"\\.\pipe\ExcaliburFleetPipe";

/// What a server is called on the board. Fleet knows installs by name, and
/// a company's Hyperview server is usually named for the company — which is
/// what its other programs on the same board are named for too. Saying which
/// program it is keeps the two from being taken for one install.
pub fn board_name(server: &str) -> String {
    let server = server.trim();
    if server.to_ascii_lowercase().contains("hyperview") {
        server.to_string()
    } else if server.is_empty() {
        "Hyperview".to_string()
    } else {
        format!("{server} (Hyperview)")
    }
}

/// Registers (or updates) the install. `Ok` carries what Fleet said.
pub fn register(name: &str, app_url: &str, key: &str) -> Result<String, String> {
    let request = serde_json::json!({
        "op": "register",
        "name": board_name(name),
        "appUrl": app_url,
        "product": "hyperview",
        "updateKey": key,
        "notes": "Excalibur Hyperview server",
    });
    let answer = ask(&request.to_string())?;
    let said: serde_json::Value =
        serde_json::from_str(&answer).map_err(|_| "Fleet's answer was not readable.".to_string())?;
    if said["ok"].as_bool() == Some(true) {
        let status = said["data"]["status"].as_str().unwrap_or("grey").to_string();
        Ok(status)
    } else {
        Err(said["error"]
            .as_str()
            .unwrap_or("Fleet said no without saying why.")
            .to_string())
    }
}

#[cfg(windows)]
fn ask(line: &str) -> Result<String, String> {
    use std::io::{BufRead, BufReader, Write};
    let pipe = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(PIPE)
        .map_err(|_| "Excalibur Fleet is not running on this computer.".to_string())?;
    (&pipe)
        .write_all(format!("{line}\n").as_bytes())
        .map_err(|e| format!("Could not reach Fleet: {e}"))?;
    let mut answer = String::new();
    BufReader::new(&pipe)
        .read_line(&mut answer)
        .map_err(|e| format!("Fleet did not answer: {e}"))?;
    Ok(answer)
}

#[cfg(not(windows))]
fn ask(_line: &str) -> Result<String, String> {
    Err("Excalibur Fleet runs on Windows, and this is not Windows.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_company_named_server_is_not_mistaken_for_the_companys_other_programs() {
        assert_eq!(board_name("Mesa Fab"), "Mesa Fab (Hyperview)");
        assert_eq!(board_name("Mesa fab Shop Server"), "Mesa fab Shop Server (Hyperview)");
        assert_eq!(board_name("Mesa Fab Hyperview"), "Mesa Fab Hyperview");
        assert_eq!(board_name("  "), "Hyperview");
    }
}
