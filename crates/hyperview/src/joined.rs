//! How a seat finds its company's server without anybody typing anything.
//!
//! The people who will use this are estimators, not administrators. They get an
//! installer in an email, they double-click it, and the program should already
//! know which server it belongs to — all that should be left is their own name
//! and password.
//!
//! So the installer writes one small file beside the program saying where the
//! company's server is, and the program reads it on first run. Build the
//! installer once per company with that address baked in and the whole
//! question disappears.
//!
//! It is a **default, not a lock.** Somebody who moves firms, or an office that
//! moves its server, can still type an address, and what they type wins from
//! then on. A program that cannot be pointed somewhere else is a program that
//! has to be reinstalled when anything changes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What the installer left behind: the company this copy belongs to.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Joined {
    /// `https://drawings.thecompany.com`
    #[serde(default)]
    pub server: String,
    /// What to call it before anybody has signed in and the server can say for
    /// itself.
    #[serde(default)]
    pub name: String,
    /// A note from whoever set it up, shown on the sign-in window. Somewhere to
    /// put "ask Dave for a login" rather than leaving somebody stuck.
    #[serde(default)]
    pub note: String,
}

impl Joined {
    pub fn set(&self) -> bool {
        !self.server.trim().is_empty()
    }

    /// Beside the program. Not in the user's own folder: it describes the
    /// installation, not the person, and every seat on a machine shares it.
    pub fn path() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join("hyperview-server.json"))
    }

    /// Reads it, if it is there. A missing or unreadable file is ordinary —
    /// somebody installed the program without a server, which is a perfectly
    /// good way to use it.
    pub fn load() -> Joined {
        // An address in the environment beats the file, which is how a shop
        // tries a new server on one machine without reinstalling.
        if let Ok(server) = std::env::var("HYPERVIEW_SERVER") {
            if !server.trim().is_empty() {
                return Joined {
                    server: server.trim().to_string(),
                    name: std::env::var("HYPERVIEW_SERVER_NAME").unwrap_or_default(),
                    note: String::new(),
                };
            }
        }
        let Some(path) = Joined::path() else {
            return Joined::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Joined::default();
        };
        let mut joined: Joined = serde_json::from_str(&text).unwrap_or_default();
        joined.server = joined.server.trim().trim_end_matches('/').to_string();
        // Only an address that could be one. A file with something odd in it
        // should leave the program usable, not pointed at nonsense.
        if !joined.server.starts_with("http://") && !joined.server.starts_with("https://") {
            joined.server.clear();
        }
        joined
    }

    /// What the sign-in window should start with: the address somebody used
    /// last, or failing that the one the installer set.
    pub fn address(&self, remembered: &str) -> String {
        if !remembered.trim().is_empty() {
            return remembered.to_string();
        }
        self.server.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Joined {
        let mut joined: Joined = serde_json::from_str(text).unwrap_or_default();
        joined.server = joined.server.trim().trim_end_matches('/').to_string();
        if !joined.server.starts_with("http://") && !joined.server.starts_with("https://") {
            joined.server.clear();
        }
        joined
    }

    #[test]
    fn a_seat_knows_its_company_server_before_anybody_types_anything() {
        let joined = read(r#"{"server":"https://drawings.acme.com","name":"Acme Steel"}"#);
        assert!(joined.set());
        assert_eq!(joined.address(""), "https://drawings.acme.com");
        assert_eq!(joined.name, "Acme Steel");
    }

    #[test]
    fn a_trailing_slash_does_not_become_a_double_one() {
        let joined = read(r#"{"server":"https://drawings.acme.com/"}"#);
        assert_eq!(joined.server, "https://drawings.acme.com");
    }

    #[test]
    fn what_somebody_typed_beats_what_the_installer_set() {
        // Because an office that moves its server should not need reinstalling
        // on six machines.
        let joined = read(r#"{"server":"https://old.acme.com"}"#);
        assert_eq!(joined.address("https://new.acme.com"), "https://new.acme.com");
    }

    #[test]
    fn a_missing_or_broken_file_leaves_a_perfectly_usable_program() {
        assert!(!read("").set());
        assert!(!read("not json at all").set());
        assert!(!read("{}").set());
        // And something that is not an address is not treated as one.
        assert!(!read(r#"{"server":"drawings.acme.com"}"#).set());
        assert!(!read(r#"{"server":"javascript:alert(1)"}"#).set());
        assert!(!read(r#"{"server":"file:///etc/passwd"}"#).set());
    }

    #[test]
    fn a_note_from_whoever_set_it_up_comes_through() {
        let joined = read(
            r#"{"server":"https://drawings.acme.com","note":"Ask Dave in the office for a login."}"#,
        );
        assert!(joined.note.contains("Ask Dave"));
    }
}
