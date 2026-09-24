//! Which edition this build is: the one from the website, or the one for the
//! Mac App Store (`--features store`).
//!
//! The App Store copy is the same program with the parts Apple's rules don't
//! allow left out, each behind one of these questions rather than scattered
//! `cfg`s, so what the store edition lacks can be read in one place:
//!
//! - It doesn't install or update itself; the store does (2.4.5 vii).
//! - It doesn't take a license key or point anywhere to buy anything (3.1.1,
//!   2.4.5 vi, 3.1.3 f). An office's license lives on its server.
//! - It doesn't run code it downloads (2.5.2), so the office server runs the
//!   office's plugins and sends back what they found.
//! - It doesn't reach into other programs' settings or the file manager's
//!   menus; the sandbox wouldn't let it (2.4.5 i).
//!
//! Everything else is the same program.

/// Built for the Mac App Store.
pub const STORE: bool = cfg!(feature = "store");

/// Installs itself, looks for new versions and puts them in place.
pub fn updates_itself() -> bool {
    !STORE
}

/// Shows prices, links to buy or renew, and takes a license file.
pub fn sells() -> bool {
    !STORE
}

/// Runs plugins on this computer. When it doesn't, the office server does.
pub fn runs_plugins_itself() -> bool {
    !STORE
}

/// Adds itself to other programs: Explorer's menus, Claude Desktop's list of
/// connectors.
pub fn reaches_other_programs() -> bool {
    !STORE
}

/// What a store copy says when asked for something it leaves to the store or
/// the office.
pub fn not_in_this_copy(what: &str) -> String {
    format!("{what} isn't part of the App Store copy of Excalibur View.")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_website_build_does_everything_and_the_store_build_leaves_out_the_same_four() {
        let asked = [
            super::updates_itself(),
            super::sells(),
            super::runs_plugins_itself(),
            super::reaches_other_programs(),
        ];
        if super::STORE {
            assert_eq!(asked, [false; 4]);
        } else {
            assert_eq!(asked, [true; 4]);
        }
    }
}
