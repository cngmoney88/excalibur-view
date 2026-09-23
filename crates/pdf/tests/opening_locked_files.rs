//! Files locked by another program, opened by this one.
//!
//! Every file here was locked by qpdf rather than by `crate::crypt`, because a
//! decryptor tested against its own encryptor proves only that it is
//! self-consistent. See `tests/locked/HOW-THESE-WERE-MADE.md`.

use pdf::opening::Lock;

const TITLE: &str = "Fox Theater Structural";
const ON_THE_SHEET: &str = "MESA FAB SHEET S-101";

/// The fixtures are kept base64-encoded, as `<name>.pdf.b64`.
///
/// Not for tidiness: this repository refuses to carry a `.pdf` at all, because
/// a drawing set belongs to whoever's project it is and one committed by
/// accident is one that ships. These are a few hundred bytes of invented text
/// and would be harmless, but the rule is worth more than the exception, so
/// they go in as text and are decoded here.
fn file(name: &str) -> pdf::Document {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/locked")
        .join(format!("{name}.b64"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    pdf::Document::from_bytes(unbase64(&text))
}

fn unbase64(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let Some(value) = ALPHABET.iter().position(|c| *c == byte) else {
            continue; // newlines and whatever else the wrapping put in
        };
        acc = (acc << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    out
}

/// The title out of the info dictionary: a string, so it exercises string
/// decryption.
fn title(doc: &pdf::Document) -> String {
    doc.info()
        .get("Title")
        .and_then(|o| o.as_text())
        .unwrap_or_default()
}

/// The first page's content stream, decoded: a stream, so it exercises stream
/// decryption and proves the filters ran on plaintext.
fn what_the_sheet_says(doc: &pdf::Document) -> String {
    let page = doc.page(0).expect("a first page");
    let contents = doc.page_attr(&page, "Contents");
    let stream = contents.as_stream().expect("a content stream");
    let bytes = pdf::filters::decode(stream).expect("it decodes");
    String::from_utf8_lossy(&bytes).to_string()
}

fn opens(name: &str, password: &str, expected: Lock) {
    let mut doc = file(name);
    assert!(doc.encrypted, "{name} should be locked");
    assert!(doc.still_locked(), "{name} should not be open yet");
    assert!(doc.unlock(password), "{name} did not open with its password");
    assert!(!doc.still_locked(), "{name} should be open now");
    assert_eq!(doc.lock(), Some(expected), "{name}: wrong sort of lock");
    assert_eq!(title(&doc), TITLE, "{name}: the title did not come back");
    assert!(
        what_the_sheet_says(&doc).contains(ON_THE_SHEET),
        "{name}: the sheet did not come back. Got: {}",
        what_the_sheet_says(&doc)
    );
}

#[test]
fn a_file_locked_with_rc4_at_forty_bits_opens() {
    opens("rc4-40.pdf", "bolt", Lock::Rc4Short);
}

#[test]
fn a_file_locked_with_rc4_at_a_hundred_and_twenty_eight_bits_opens() {
    opens("rc4-128.pdf", "bolt", Lock::Rc4Long);
}

#[test]
fn a_file_locked_with_aes_128_opens() {
    opens("aes-128.pdf", "bolt", Lock::Aes128);
}

#[test]
fn a_file_locked_with_aes_256_opens() {
    opens("aes-256.pdf", "bolt", Lock::Aes256);
}

#[test]
fn the_owners_password_opens_it_too() {
    // It has to. The owner is the person who set the permissions, and a
    // drawing set nobody can get back into is a drawing set nobody can fix.
    for name in ["rc4-40.pdf", "rc4-128.pdf", "aes-128.pdf", "aes-256.pdf"] {
        let mut doc = file(name);
        assert!(doc.unlock("creede"), "{name} did not open with the owner's password");
        assert_eq!(title(&doc), TITLE, "{name}: opened, but read as rubbish");
    }
}

#[test]
fn a_file_with_only_an_owners_password_opens_with_nothing_typed() {
    // The commonest shape in circulation: anybody may open it, and the file
    // asks that nobody print it.
    let mut doc = file("owner-only.pdf");
    assert!(doc.encrypted);
    assert!(doc.unlock(""), "it should open with an empty password");
    assert_eq!(title(&doc), TITLE);
    let allowed = doc.allowed().expect("permissions");
    assert!(!allowed.print, "printing was denied when it was locked");
    assert!(allowed.copy, "only printing was denied");
}

#[test]
fn the_wrong_password_does_not_open_anything() {
    for name in ["rc4-40.pdf", "rc4-128.pdf", "aes-128.pdf", "aes-256.pdf"] {
        let mut doc = file(name);
        assert!(!doc.unlock("bolts"), "{name} opened with the wrong password");
        assert!(!doc.unlock("BOLT"), "{name} is not case insensitive");
        assert!(!doc.unlock(""), "{name} opened with no password at all");
        assert!(doc.still_locked(), "{name} should still be shut");
        assert!(doc.lock().is_none());
    }
}

#[test]
fn what_is_read_before_the_password_is_rubbish_and_not_mistaken_for_text() {
    // The point of `still_locked`: a caller that reads without opening gets
    // ciphertext, and must not put it on screen as if it were a title.
    let doc = file("aes-256.pdf");
    assert!(doc.still_locked());
    assert_ne!(title(&doc), TITLE);
}

#[test]
fn only_aes_256_counts_as_a_real_lock() {
    for (name, weak) in [
        ("rc4-40.pdf", true),
        ("rc4-128.pdf", true),
        ("aes-128.pdf", true),
        ("aes-256.pdf", false),
    ] {
        let mut doc = file(name);
        assert!(doc.unlock("bolt"));
        assert_eq!(doc.lock().unwrap().weak(), weak, "{name}");
    }
}

#[test]
fn an_unlocked_file_is_already_open() {
    let mut doc = file("plain.pdf");
    assert!(!doc.encrypted);
    assert!(!doc.still_locked());
    // Asking for a password on a file that has none is not an error: the
    // caller wanted a readable document and has one.
    assert!(doc.unlock("anything at all"));
    assert_eq!(title(&doc), TITLE);
}
