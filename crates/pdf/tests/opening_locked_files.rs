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

// ---- writing back into one --------------------------------------------

/// Adds an object carrying a string and a stream, the way a markup does, and
/// hands back the whole file.
fn add_something(doc: &pdf::Document, says: &str) -> (Vec<u8>, pdf::Ref) {
    let mut update = pdf::write::Update::new(doc);
    let mut dict = pdf::Dict::new();
    dict.set(pdf::Name::new("Type"), pdf::Object::Name(pdf::Name::new("Annot")));
    dict.set(pdf::Name::new("Subj"), pdf::Object::String(says.as_bytes().to_vec(), pdf::StringKind::Literal));
    let stream = pdf::Stream {
        dict: {
            let mut d = pdf::Dict::new();
            d.set(pdf::Name::new("Length"), pdf::Object::Int(says.len() as i64));
            d
        },
        data: says.as_bytes().to_vec(),
    };
    dict.set(pdf::Name::new("AP"), pdf::Object::Stream(Box::new(stream)));
    let at = update.add(pdf::Object::Dict(dict));
    (update.apply(doc), at)
}

#[test]
fn a_markup_written_into_a_locked_set_is_locked_too() {
    // The whole point. A markup added to a locked file used to go in as plain
    // text behind a file declaring everything encrypted, which is a drawing
    // set no reader opens.
    for name in ["rc4-40.pdf", "rc4-128.pdf", "aes-128.pdf", "aes-256.pdf"] {
        let mut doc = file(name);
        assert!(doc.unlock("bolt"), "{name}");
        let (bytes, at) = add_something(&doc, "W12x26 typical");

        // Read it back the way anybody else would: from the bytes, with the
        // password and nothing else.
        let mut again = pdf::Document::from_bytes(bytes.clone());
        assert!(again.encrypted, "{name}: it should still say it is locked");
        assert!(again.unlock("bolt"), "{name}: it should still open");
        let added = again.get(at);
        let dict = added.as_dict().unwrap_or_else(|| panic!("{name}: the object went missing"));
        assert_eq!(
            dict.get("Subj").and_then(|o| o.as_text()).as_deref(),
            Some("W12x26 typical"),
            "{name}: the string did not survive"
        );
        let ap = dict.get("AP").and_then(|o| o.as_stream()).expect("the appearance");
        assert_eq!(ap.data, b"W12x26 typical", "{name}: the stream did not survive");

        // And the rest of the file still reads, which is what says the update
        // did not disturb what was already there.
        assert_eq!(title(&again), TITLE, "{name}: the original went wrong");
        assert!(what_the_sheet_says(&again).contains(ON_THE_SHEET), "{name}");
    }
}

#[test]
fn what_was_written_is_not_sitting_there_in_plain_sight() {
    // The failure this guards against is silent: the file opens, the markup
    // reads back, and the words are also legible to anybody with a hex
    // editor. Checked against the bytes rather than against the reader.
    let mut doc = file("aes-256.pdf");
    assert!(doc.unlock("bolt"));
    let (bytes, _) = add_something(&doc, "MESA FAB CONFIDENTIAL");
    let haystack = String::from_utf8_lossy(&bytes);
    assert!(
        !haystack.contains("MESA FAB CONFIDENTIAL"),
        "the markup went into a locked file as plain text"
    );
}

#[test]
fn the_same_words_saved_twice_are_not_written_the_same_way_twice() {
    // AES in CBC needs an initialisation vector nobody can predict. If two
    // saves of the same markup produced the same bytes, that would say the
    // vector was a counter or a constant.
    let mut doc = file("aes-256.pdf");
    assert!(doc.unlock("bolt"));
    let (once, _) = add_something(&doc, "the same words");
    let (twice, _) = add_something(&doc, "the same words");
    assert_ne!(once, twice, "the same plaintext encrypted to the same bytes");
}

#[test]
fn an_unlocked_file_is_written_exactly_as_it_was_before() {
    // Nothing above may change what happens to an ordinary drawing set.
    let doc = file("plain.pdf");
    assert!(doc.sealing().is_none());
    let (bytes, at) = add_something(&doc, "W12x26 typical");
    let again = pdf::Document::from_bytes(bytes);
    let added = again.get(at);
    let dict = added.as_dict().expect("the object");
    assert_eq!(
        dict.get("Subj").and_then(|o| o.as_text()).as_deref(),
        Some("W12x26 typical")
    );
}
