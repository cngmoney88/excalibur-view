//! Which toolbars and which tools a seat has.
//!
//! Excalibur View keeps tools in its own `.evtools` files (`chest::native`).
//! Files that came out of Revu are still read, so a seat set up before that
//! keeps working; anything loaded now is kept in Excalibur View's own format.
//!
//! Two kinds of file come out of Revu. A profile (`.bpx`) is a whole way of
//! working — toolbars, panels, the markups list's columns, and a tool chest. A
//! tool set (`.btx`) is tools and nothing else. Offices share both, and a seat
//! usually has more than one lying about: the office's profile from the server,
//! plus a tool set or two somebody emailed round.
//!
//! So a seat's arrangement is assembled, not picked:
//!
//! - **The layout** — toolbars, panels, columns — comes from the newest profile
//!   that has toolbars in it. Newest, so when an administrator shares a new
//!   office profile it takes over on every seat without anybody choosing it.
//! - **The tools** are every tool set in every file, each set once. Loading a
//!   `.btx` adds its tools; it never takes the toolbars away, which is what
//!   loading one used to do.
//! - With no profile at all, the layout is Hyperview's standard one, which has
//!   every tool on it. Never an empty bar and never a cut-down one.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::SystemTime;

use sha2::{Digest, Sha256};

/// Where profiles and tool sets are looked for, in order.
pub fn places() -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = Vec::new();
    if let Ok(from) = std::env::var("HYPERVIEW_PROFILE") {
        places.push(PathBuf::from(from));
    }
    places.push(crate::server::profiles_folder());
    if let Some(dirs) = directories::ProjectDirs::from("", "", "Excalibur Hyperview") {
        places.push(dirs.data_dir().join("profiles"));
    }
    places.dedup();
    places
}

/// Every profile and tool set file in those places.
pub fn files() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for place in places() {
        if place.is_file() {
            found.push(place);
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&place) else {
            continue;
        };
        let mut here: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| is_chest_file(p))
            .collect();
        here.sort();
        found.extend(here);
    }
    found
}

fn is_chest_file(path: &std::path::Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .as_deref(),
        Some("evtools") | Some("bpx") | Some("btx")
    )
}

/// The seat's arrangement, assembled from every file there is. `None` only
/// when there are no files at all.
pub fn load() -> Option<chest::Profile> {
    let read: Vec<(SystemTime, chest::Profile)> = files()
        .into_iter()
        .filter_map(|path| {
            let when = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            chest::Profile::open(&path).map(|p| (when, p))
        })
        .collect();
    assemble(read)
}

/// The assembling, apart from the reading, so it can be tested.
pub fn assemble(mut read: Vec<(SystemTime, chest::Profile)>) -> Option<chest::Profile> {
    if read.is_empty() {
        return None;
    }
    // Newest first: its layout, and its version of any set two files share.
    read.sort_by(|a, b| b.0.cmp(&a.0));
    let layout_at = read
        .iter()
        .position(|(_, p)| ui::chrome::has_toolbars(&p.toolbars))
        .unwrap_or(0);
    let mut all = read.into_iter().map(|(_, p)| p).collect::<Vec<_>>();
    let mut profile = all.remove(layout_at);
    let mut seen: HashSet<String> = profile.sets.iter().map(|s| s.title.clone()).collect();
    for other in all {
        for set in other.sets {
            if seen.insert(set.title.clone()) {
                profile.sets.push(set);
            }
        }
    }
    Some(profile)
}

/// The digests of every profile and tool set this seat already has, so one
/// the office shares is fetched once and not again at every sign-in.
pub fn local_digests() -> HashSet<String> {
    files()
        .into_iter()
        .filter_map(|path| std::fs::read(path).ok())
        .map(|bytes| hex(&Sha256::digest(&bytes)))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn tools(title: &str, n: usize) -> chest::ToolSet {
        chest::ToolSet {
            title: title.into(),
            tools: (0..n)
                .map(|i| chest::toolset::plain(&format!("{title} {i}"), chest::Kind::Length))
                .collect(),
        }
    }

    fn profile(name: &str, toolbars: bool, sets: Vec<chest::ToolSet>) -> chest::Profile {
        let mut p = chest::Profile::read(b"<RevuProfile Name=\"x\"></RevuProfile>")
            .expect("an empty profile reads");
        p.name = name.into();
        p.toolbars = if toolbars { ui::chrome::standard_toolbars() } else { Vec::new() };
        p.sets = sets;
        p
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn a_tool_set_loaded_after_a_profile_adds_tools_and_keeps_the_toolbars() {
        let office = profile("Ron Quantity Take Off", true, vec![tools("Beams", 3)]);
        let emailed = profile("Counters", false, vec![tools("Counters", 2)]);
        let seat = assemble(vec![(at(10), office), (at(20), emailed)]).unwrap();
        assert_eq!(seat.name, "Ron Quantity Take Off");
        assert!(ui::chrome::has_toolbars(&seat.toolbars));
        let titles: Vec<&str> = seat.sets.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, vec!["Beams", "Counters"]);
    }

    #[test]
    fn the_newest_profile_with_toolbars_sets_the_layout() {
        let mine = profile("Mine", true, vec![tools("Beams", 1)]);
        let office = profile("Office", true, vec![tools("Beams", 5), tools("Columns", 2)]);
        let seat = assemble(vec![(at(10), mine), (at(30), office)]).unwrap();
        assert_eq!(seat.name, "Office");
        // The newer file's version of a set both have.
        assert_eq!(seat.sets.iter().find(|s| s.title == "Beams").unwrap().tools.len(), 5);
        assert_eq!(seat.sets.len(), 2);
    }

    #[test]
    fn tool_sets_alone_still_give_every_tool_in_every_set() {
        let a = profile("A", false, vec![tools("A", 1)]);
        let b = profile("B", false, vec![tools("B", 1)]);
        let seat = assemble(vec![(at(1), a), (at(2), b)]).unwrap();
        assert_eq!(seat.sets.len(), 2);
        // No toolbars in it: the window uses the standard ones.
        assert!(!ui::chrome::has_toolbars(&seat.toolbars));
    }

    #[test]
    fn nothing_at_all_is_nothing() {
        assert!(assemble(Vec::new()).is_none());
    }
}
