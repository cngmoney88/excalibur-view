//! The release manifest the publishing script makes, checked by the code a
//! seat and a server run.
//!
//! Signing happens in `tools/publish.py`, on the publisher's own computer,
//! because that is where the key is. Checking happens here, in Rust, on every
//! machine in every office. If the two ever disagree by a single byte about
//! what was signed, every update everywhere is refused — so this reads a
//! manifest the script really made and checks it the way a seat does.
//!
//! The key is a throwaway, the same one the server's own update tests use.

use ed25519_dalek::SigningKey;
use hub::feed::{Published, SERVER_PLATFORM};
use hub::update::{Channel, Refusal, Trusted};

fn trusted() -> Trusted {
    let key = SigningKey::from_bytes(&[42u8; 32]);
    Trusted {
        keys: vec![("test-2026".into(), key.verifying_key().to_bytes())],
    }
}

fn manifest() -> Published {
    serde_json::from_str(include_str!("fixtures/python-signed-release.json"))
        .expect("the script's manifest reads as the feed's own type")
}

#[test]
fn what_the_script_signs_is_what_a_seat_checks() {
    let published = manifest();
    // The fixture is a Windows release whatever machine this test runs on, so
    // it is read against the Windows names rather than this build's own.
    assert_eq!(published.app.platform, "windows-x64");
    assert_eq!(published.app.channel, Channel::Preview);
    assert_eq!(published.app.download, "Hyperview.exe");
    assert_eq!(
        trusted().check(&published.app, b"MZ pretend viewer 9.1.0"),
        Ok(())
    );
    let server = published.server.expect("the server is in it too");
    assert_eq!(server.platform, "windows-x64-server");
    assert_eq!(server.download, "Hyperview-Server.exe");
    assert_eq!(trusted().check(&server, b"MZ pretend server 9.1.0"), Ok(()));
}

/// The two pretend programs the fixture was signed over, the same length as
/// each other so that telling them apart can only be the fingerprint's doing.
const WINDOWS: &[u8] = b"MZ pretend viewer 9.4.0";
const MAC: &[u8] = b"PK pretend mac 9.4.0...";

/// This release was made by the script as it stands, with a Mac build in it.
fn two_platforms() -> Published {
    serde_json::from_str(include_str!("fixtures/python-signed-two-platforms.json"))
        .expect("a release carrying two builds reads as the feed's own type")
}

#[test]
fn both_builds_in_one_release_check_out_and_neither_is_the_other() {
    let published = two_platforms();

    let windows = published.app_for("windows-x64").expect("the Windows build");
    assert_eq!(windows.download, "ExcaliburView.exe");
    assert_eq!(trusted().check(windows, WINDOWS), Ok(()));

    let mac = published.app_for("macos-universal").expect("the Mac build");
    // The zip and not the disk image: an update unpacks it, a person does not.
    assert_eq!(mac.download, "ExcaliburView-mac.zip");
    assert_eq!(trusted().check(mac, MAC), Ok(()));

    // Each signature is over its own file, and the two are the same length
    // here on purpose: what refuses them is the fingerprint of the contents,
    // not a size that happened not to match. This is what stops a feed handing
    // a Mac the Windows program and calling it an update.
    assert!(matches!(
        trusted().check(mac, WINDOWS),
        Err(Refusal::WrongDigest { .. })
    ));
    assert!(matches!(
        trusted().check(windows, MAC),
        Err(Refusal::WrongDigest { .. })
    ));
}

/// The point of keeping `app` where it was: a copy that never heard of `apps`
/// still finds the Windows build and still updates.
#[test]
fn a_copy_that_only_knows_the_old_shape_still_finds_its_update() {
    let published = two_platforms();
    assert_eq!(published.app.platform, "windows-x64");
    assert_eq!(trusted().check(&published.app, WINDOWS), Ok(()));
}

/// And a Mac cannot be talked into installing the Windows build by a manifest
/// that names the wrong platform beside the right signature.
#[test]
fn a_build_relabelled_as_another_platform_is_refused() {
    let published = two_platforms();
    let mut relabelled = published.app_for("windows-x64").unwrap().clone();
    relabelled.platform = "macos-universal".into();
    assert_eq!(
        trusted().check(&relabelled, WINDOWS),
        Err(Refusal::BadSignature)
    );
}

#[test]
fn and_a_manifest_edited_after_signing_is_refused() {
    // Somebody changes the channel by hand to push an early release to every
    // office. The channel is part of what was signed.
    let mut published = manifest();
    published.app.channel = Channel::Stable;
    assert_eq!(
        trusted().check(&published.app, b"MZ pretend viewer 9.1.0"),
        Err(Refusal::BadSignature)
    );
}

#[test]
fn and_the_viewers_signature_cannot_be_passed_off_as_the_servers() {
    let published = manifest();
    let mut swapped = published.app.clone();
    swapped.platform = SERVER_PLATFORM.into();
    assert_eq!(
        trusted().check(&swapped, b"MZ pretend viewer 9.1.0"),
        Err(Refusal::BadSignature)
    );
}
