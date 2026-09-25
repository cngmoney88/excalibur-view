//! Sealed: every connection to the outside world, off.
//!
//! A fabricator working on Controlled Unclassified Information has to be able
//! to say where their drawings go, and "nowhere" is the only answer that ends
//! the conversation. Sealed is how the program makes that true rather than
//! promised.
//!
//! **What sealed switches off** is everything that leaves the company's own
//! network: the update check, the release feed, the assistant, Excalibur
//! Fleet, and any link the program would otherwise open in a browser. What it
//! leaves alone is everything inside the network — the office server, the
//! drawings, the markups, the takeoff. A sealed seat is not a crippled seat;
//! it is the same program with the doors to the outside welded shut.
//!
//! **Two things can seal an install, and neither is a checkbox in the app:**
//!
//! 1. **The license.** An `edition: "sealed"` license seals every seat that
//!    signs in to that server, and the server itself. Nobody has to remember
//!    to switch anything on, and nobody in the office can switch it off.
//! 2. **Machine policy.** An administrator sets a value under
//!    `HKLM\Software\Policies\Excalibur\View`, through Group Policy, and that
//!    machine is sealed before anybody signs in to anything — which is what
//!    an IT department actually needs, because a computer that has never
//!    reached the office server is exactly the computer they are worried
//!    about.
//!
//! Policy wins. A machine sealed by policy stays sealed on an ordinary Office
//! license, on no license at all, and while signed out. The reverse is not
//! true and must never be: no setting, license or registry value unseals a
//! machine that policy sealed.
//!
//! **Why it is not a preference.** A preference is a thing somebody turns off
//! at 4pm on a Friday to get a download working. The whole value of Sealed is
//! that a compliance officer can be told, truthfully, that there is no such
//! switch.

use std::sync::atomic::{AtomicU8, Ordering};

/// Why this install is sealed, if it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sealed {
    /// Ordinary. The outside world is reachable.
    No,
    /// The office's license is a Sealed license.
    ByLicense,
    /// This computer is sealed by its administrator, whatever the license
    /// says. Cannot be undone from inside the program.
    ByPolicy,
}

impl Sealed {
    pub fn is_sealed(self) -> bool {
        self != Sealed::No
    }

    /// One sentence for whoever is looking at it.
    pub fn why(self) -> &'static str {
        match self {
            Sealed::No => "",
            Sealed::ByLicense => {
                "Sealed: this office's license switches off every connection to the outside world."
            }
            Sealed::ByPolicy => {
                "Sealed by your administrator: every connection to the outside world is off on \
                 this computer, and cannot be turned on from here."
            }
        }
    }
}

const UNKNOWN: u8 = 0;
const OPEN: u8 = 1;
const BY_LICENSE: u8 = 2;
const BY_POLICY: u8 = 3;

/// Worked out once and then held, because it is asked on every outbound call
/// and must not depend on a registry read succeeding twice running.
static STATE: AtomicU8 = AtomicU8::new(UNKNOWN);

fn from_code(code: u8) -> Sealed {
    match code {
        BY_POLICY => Sealed::ByPolicy,
        BY_LICENSE => Sealed::ByLicense,
        _ => Sealed::No,
    }
}

/// How this install stands. Reads machine policy the first time and remembers
/// it; a policy that arrives later takes effect when the program next starts,
/// which is how Group Policy behaves for everything else.
pub fn state() -> Sealed {
    let held = STATE.load(Ordering::Relaxed);
    if held != UNKNOWN {
        return from_code(held);
    }
    let code = if policy_seals_this_machine() { BY_POLICY } else { OPEN };
    // A race here is harmless: both threads worked out the same answer.
    STATE.store(code, Ordering::Relaxed);
    from_code(code)
}

pub fn is_sealed() -> bool {
    state().is_sealed()
}

/// Called when the office server says what edition its license is. A Sealed
/// license seals this seat; anything else leaves policy as the only thing
/// that can, and never unseals a machine policy sealed.
pub fn license_says(edition: Option<&str>) {
    if state() == Sealed::ByPolicy {
        return;
    }
    let sealed = edition.map(|e| e.eq_ignore_ascii_case(SEALED_EDITION)) == Some(true);
    STATE.store(if sealed { BY_LICENSE } else { OPEN }, Ordering::Relaxed);
}

/// The edition string on a Sealed license. The same word the license itself
/// carries, so the two can never drift apart.
pub const SEALED_EDITION: &str = crate::license::Edition::Sealed.name();

/// For tests, and for a server that is told what it is rather than reading a
/// registry it does not have.
pub fn set_for_testing(state: Sealed) {
    STATE.store(
        match state {
            Sealed::No => OPEN,
            Sealed::ByLicense => BY_LICENSE,
            Sealed::ByPolicy => BY_POLICY,
        },
        Ordering::Relaxed,
    );
}

/// What an outbound call gets instead of a connection.
pub fn refusal(what: &str) -> String {
    format!(
        "{what} needs a connection to the outside world, and this installation is sealed. \
         {} Everything inside your own network still works as it always did.",
        state().why()
    )
}

/// Whether a URL is somewhere inside the company, and so still allowed when
/// sealed. Anything that is not plainly a private address is treated as the
/// outside world: guessing wrong in that direction is the harmless one.
pub fn is_inside(url: &str) -> bool {
    let rest = match url.split_once("://") {
        Some((_, rest)) => rest,
        None => url,
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("");
    // Strip a port, carefully: an IPv6 literal is in brackets.
    let host = if let Some(end) = host.strip_prefix('[') {
        end.split(']').next().unwrap_or("")
    } else {
        host.split(':').next().unwrap_or("")
    };
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return false;
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    if host == "::1" {
        return true;
    }
    // A name with no dot in it is a machine on the office network: the server
    // a shop reaches as `drawings` rather than `drawings.example.com`.
    if !host.contains('.') && !host.contains(':') {
        return true;
    }
    if let Some(rest) = host.strip_suffix('.') {
        return is_inside(rest);
    }
    for suffix in [".local", ".lan", ".internal", ".intranet", ".home.arpa"] {
        if host.ends_with(suffix) {
            return true;
        }
    }
    private_address(&host)
}

/// The address ranges RFC 1918 and friends set aside for private networks.
fn private_address(host: &str) -> bool {
    let parts: Vec<&str> = host.split('.').collect();
    if parts.len() != 4 {
        return false;
    }
    let mut octets = [0u16; 4];
    for (i, part) in parts.iter().enumerate() {
        match part.parse::<u16>() {
            Ok(n) if n <= 255 && !part.is_empty() => octets[i] = n,
            _ => return false,
        }
    }
    match octets {
        [10, _, _, _] => true,
        [127, _, _, _] => true,
        [192, 168, _, _] => true,
        [169, 254, _, _] => true,
        [172, second, _, _] if (16..=31).contains(&second) => true,
        _ => false,
    }
}

#[cfg(windows)]
fn policy_seals_this_machine() -> bool {
    // HKLM\Software\Policies\Excalibur\View, value `Sealed` (DWORD), non-zero.
    // Under Policies, which is the branch Group Policy owns and ordinary users
    // cannot write to.
    windows_policy::dword("Software\\Policies\\Excalibur\\View", "Sealed").unwrap_or(0) != 0
}

/// macOS has no registry and no Group Policy. Its equivalent is a **managed
/// preferences** file, pushed by whatever MDM the company runs, which lands in
/// `/Library/Managed Preferences/` and which an ordinary user cannot write —
/// the same property that makes the Windows Policies branch worth trusting.
///
/// The file is read directly rather than through `defaults`, so this works on
/// a machine with nothing installed and does not depend on a command existing.
#[cfg(target_os = "macos")]
fn policy_seals_this_machine() -> bool {
    const MANAGED: &str = "/Library/Managed Preferences/com.excaliburct.view.plist";
    // Also the plain preferences domain, for an administrator setting it with
    // `sudo defaults write` on one machine rather than through an MDM.
    const LOCAL: &str = "/Library/Preferences/com.excaliburct.view.plist";
    [MANAGED, LOCAL].iter().any(|path| plist_says_sealed(path))
}

/// Whether a preferences file sets `Sealed` to something true.
///
/// Only a Mac has managed preferences, so this is only compiled there. What
/// it does with the bytes once it has them is `sealed_in_plist`, which is
/// compiled and tested everywhere -- the part worth checking does not need a
/// Mac to check it on.
#[cfg(target_os = "macos")]
///
/// Reads both shapes a plist comes in: the XML one, and the binary one the
/// system writes. The binary reader is deliberately simple — it looks for the
/// key and the boolean beside it rather than parsing the whole format —
/// because getting this wrong in the unsafe direction is not acceptable and
/// the safe failure is "not sealed by policy", which leaves the license as the
/// only thing that seals. An administrator who wants certainty writes XML.
fn plist_says_sealed(path: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    sealed_in_plist(&bytes)
}

/// The same question, asked of bytes rather than a path, so it can be tested
/// without a Mac.
#[cfg(any(target_os = "macos", test))]
fn sealed_in_plist(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"bplist00") {
        // In a binary plist a boolean is a one-byte marker: 0x08 false,
        // 0x09 true. The key is stored as an ASCII string near it.
        let Some(at) = find(bytes, b"Sealed") else { return false };
        return bytes[at + 6..].iter().take(64).any(|b| *b == 0x09);
    }
    let text = String::from_utf8_lossy(bytes);
    let Some(at) = text.find("<key>Sealed</key>") else {
        return false;
    };
    let after = &text[at + "<key>Sealed</key>".len()..];
    let after = after.trim_start();
    after.starts_with("<true/>")
        || after.starts_with("<true />")
        || after
            .strip_prefix("<integer>")
            .and_then(|rest| rest.split('<').next())
            .map(|n| n.trim() != "0" && !n.trim().is_empty())
            .unwrap_or(false)
}

#[cfg(any(target_os = "macos", test))]
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn policy_seals_this_machine() -> bool {
    // A server on Linux has neither a registry nor managed preferences. It is
    // sealed by its license, or by the environment it is started with.
    std::env::var("HYPERVIEW_SEALED")
        .map(|v| {
            let v = v.trim();
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        })
        .unwrap_or(false)
}

#[cfg(windows)]
mod windows_policy {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_READ,
        REG_VALUE_TYPE,
    };

    pub fn dword(path: &str, name: &str) -> Option<u32> {
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                &HSTRING::from(path),
                0,
                KEY_READ,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let mut value: u32 = 0;
            let mut size: u32 = std::mem::size_of::<u32>() as u32;
            let mut kind = REG_VALUE_TYPE(0);
            let read = RegQueryValueExW(
                key,
                &HSTRING::from(name),
                None,
                Some(&mut kind),
                Some(&mut value as *mut u32 as *mut u8),
                Some(&mut size),
            );
            let _ = RegCloseKey(key);
            (read == ERROR_SUCCESS).then_some(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The state these tests move is global to the process, so they take it in
    /// turns rather than racing each other.
    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn alone() -> std::sync::MutexGuard<'static, ()> {
        ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner())
    }


    #[test]
    fn a_managed_preference_can_seal_a_mac() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
  <key>Sealed</key>
  <true/>
</dict></plist>"#;
        assert!(sealed_in_plist(xml));
    }

    #[test]
    fn a_mac_left_alone_is_not_sealed_by_policy() {
        for not_sealed in [
            &b"<plist><dict><key>Sealed</key><false/></dict></plist>"[..],
            &b"<plist><dict><key>Something Else</key><true/></dict></plist>"[..],
            &b"<plist><dict></dict></plist>"[..],
            &b""[..],
        ] {
            assert!(!sealed_in_plist(not_sealed), "{}", String::from_utf8_lossy(not_sealed));
        }
    }

    #[test]
    fn an_administrator_may_write_it_as_a_number() {
        assert!(sealed_in_plist(b"<plist><dict><key>Sealed</key><integer>1</integer></dict></plist>"));
        assert!(!sealed_in_plist(b"<plist><dict><key>Sealed</key><integer>0</integer></dict></plist>"));
    }

    #[test]
    fn a_binary_plist_is_read_too() {
        // How the system writes it: the key as ASCII, the true marker after.
        let mut bytes = b"bplist00".to_vec();
        bytes.extend_from_slice(b"\x56Sealed");
        bytes.push(0x09);
        assert!(sealed_in_plist(&bytes));

        let mut off = b"bplist00".to_vec();
        off.extend_from_slice(b"\x56Sealed");
        off.push(0x08);
        assert!(!sealed_in_plist(&off), "0x08 is false");
    }

    #[test]
    fn something_that_is_not_a_plist_at_all_seals_nothing() {
        assert!(!sealed_in_plist(b"\x89PNG\r\n\x1a\n"));
        assert!(!sealed_in_plist(&[0xff; 512]));
    }

    #[test]
    fn policy_outranks_a_license() {
        let _alone = alone();
        set_for_testing(Sealed::ByPolicy);
        // An ordinary Office license arriving does not unseal the machine.
        license_says(Some("office"));
        assert_eq!(state(), Sealed::ByPolicy);
        license_says(None);
        assert_eq!(state(), Sealed::ByPolicy);
        set_for_testing(Sealed::No);
    }

    #[test]
    fn a_sealed_license_seals_the_seat() {
        let _alone = alone();
        set_for_testing(Sealed::No);
        license_says(Some("sealed"));
        assert!(is_sealed());
        assert_eq!(state(), Sealed::ByLicense);
        // And an office license afterwards opens it again: the office changed
        // what it bought, which is theirs to do.
        license_says(Some("office"));
        assert!(!is_sealed());
        set_for_testing(Sealed::No);
    }

    #[test]
    fn the_edition_is_read_however_it_is_spelled() {
        let _alone = alone();
        set_for_testing(Sealed::No);
        license_says(Some("Sealed"));
        assert!(is_sealed());
        set_for_testing(Sealed::No);
    }

    #[test]
    fn the_office_network_is_not_the_outside_world() {
        for inside in [
            "http://drawings/",
            "https://drawings.local/health",
            "http://192.168.1.50:8080/sets",
            "http://10.0.0.7/",
            "https://172.16.4.9/license",
            "http://172.31.255.255/",
            "http://localhost:7777/",
            "http://127.0.0.1:7777/health",
            "https://shop.lan/",
            "https://files.internal/",
            "http://[::1]:7777/",
        ] {
            assert!(is_inside(inside), "{inside} is inside the company");
        }
    }

    #[test]
    fn everything_else_is_the_outside_world() {
        for outside in [
            "https://api.github.com/repos/x/y/releases",
            "https://excaliburct.com/pricing/",
            "https://api.anthropic.com/v1/messages",
            "http://172.15.0.1/",
            "http://172.32.0.1/",
            "http://11.0.0.1/",
            "https://192.169.1.1/",
            "https://example.com/",
            "",
        ] {
            assert!(!is_inside(outside), "{outside} is outside the company");
        }
    }

    #[test]
    fn a_refusal_says_what_it_was_and_what_still_works() {
        let _alone = alone();
        set_for_testing(Sealed::ByPolicy);
        let said = refusal("Checking for updates");
        assert!(said.contains("Checking for updates"));
        assert!(said.contains("administrator"));
        assert!(said.contains("inside your own network still works"));
        set_for_testing(Sealed::No);
    }
}

#[cfg(test)]
mod what_a_sealed_seat_may_still_reach {
    //! A sealed seat is a seat with the doors to the internet shut, not a
    //! seat that cannot work. The office server is not the outside world.
    //!
    //! What these pin down is the edge that remote access created: a server
    //! with a tunnel answers on a public hostname, so "the office server" and
    //! "a machine on the office network" stopped being the same thing.

    use super::is_inside;

    #[test]
    fn the_office_server_on_the_office_network_is_always_reachable() {
        for inside in [
            "http://192.168.1.20:8714",
            "http://10.0.0.5:8714",
            "http://172.16.4.9:8714",
            "http://drawings:8714",
            "http://drawings.local:8714",
            "http://drawings.lan",
            "http://drawings.internal",
            "http://localhost:8714",
            "http://[::1]:8714",
        ] {
            assert!(is_inside(inside), "{inside} should count as the office network");
        }
    }

    #[test]
    fn the_same_server_on_a_public_hostname_does_not() {
        // This is the one that matters. A shop turns remote access on, the
        // server answers at drawings.theirshop.com, and a seat pointed there
        // is reaching across the internet however familiar the name looks.
        for outside in [
            "https://drawings.mesafab.com",
            "https://drawings.mesafab.com/mcp",
            "https://excaliburct.com/f/x.evlicense",
            "https://api.github.com/repos/x/y",
            "http://8.8.8.8",
        ] {
            assert!(!is_inside(outside), "{outside} should not count as inside");
        }
    }

    #[test]
    fn a_public_address_dressed_up_as_a_private_one_is_still_public() {
        // The shapes somebody would try if they wanted past this.
        for dressed in [
            "https://192.168.1.20.evil.com",
            "https://user@evil.com",
            "https://evil.com:8714",
            "https://drawings.local.evil.com",
        ] {
            assert!(!is_inside(dressed), "{dressed} slipped through as inside");
        }
    }

    #[test]
    fn a_trailing_dot_does_not_get_anybody_in() {
        assert!(!is_inside("https://evil.com."));
        assert!(is_inside("http://drawings.local."));
    }
}
