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
use hub::feed::{Published, APP_PLATFORM, SERVER_PLATFORM};
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
    assert_eq!(published.app.platform, APP_PLATFORM);
    assert_eq!(published.app.channel, Channel::Preview);
    assert_eq!(published.app.download, "Hyperview.exe");
    assert_eq!(
        trusted().check(&published.app, b"MZ pretend viewer 9.1.0"),
        Ok(())
    );
    let server = published.server.expect("the server is in it too");
    assert_eq!(server.platform, SERVER_PLATFORM);
    assert_eq!(server.download, "Hyperview-Server.exe");
    assert_eq!(trusted().check(&server, b"MZ pretend server 9.1.0"), Ok(()));
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
