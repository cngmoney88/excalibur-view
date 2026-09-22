//! Whether to believe a plugin file.
//!
//! The same rule as an update, for the same reason: a server hands plugins
//! to every seat in an office, and running a server must not be enough to
//! decide what runs on those seats. A plugin is taken only when the
//! WebAssembly inside is exactly what one of the program's trusted keys
//! signed. The sandbox is the second wall, not the only one.

use plugin_api::Header;
use sha2::{Digest, Sha256};

use crate::update::{hex, unhex, verify, Trusted};

/// A plugin file that passed.
#[derive(Clone, Debug)]
pub struct Checked {
    pub header: Header,
    pub wasm: Vec<u8>,
    /// SHA-256 of the whole file, which is how the office server and each
    /// seat tell whether they hold the same one.
    pub digest: String,
}

pub fn check(trusted: &Trusted, file: &[u8]) -> Result<Checked, String> {
    let (header, wasm) = plugin_api::open(file)?;
    if header.id.is_empty()
        || !header
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("A plugin can't be called '{}'.", header.id));
    }
    let found = hex(&Sha256::digest(wasm));
    if found != header.sha256.to_lowercase() {
        return Err(format!(
            "Plugin '{}' did not arrive whole. It has not been loaded.",
            header.id
        ));
    }
    let Some(key) = trusted.key(&header.key) else {
        return Err(format!(
            "Plugin '{}' is signed with a key this program does not know ({}). It has not been loaded.",
            header.id, header.key
        ));
    };
    let signature = unhex(&header.signature).ok_or_else(|| {
        format!("Plugin '{}' has a signature that can't be read.", header.id)
    })?;
    verify(
        key,
        &plugin_api::signing_payload(&header.id, &header.version, &found),
        &signature,
    )
    .map_err(|_| {
        format!(
            "Plugin '{}' is not signed by its publisher. It has not been loaded.",
            header.id
        )
    })?;
    Ok(Checked {
        header,
        wasm: wasm.to_vec(),
        digest: hex(&Sha256::digest(file)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn publisher() -> (SigningKey, Trusted) {
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let trusted = Trusted {
            keys: vec![("mesafab-2026".into(), signing.verifying_key().to_bytes())],
        };
        (signing, trusted)
    }

    pub fn signed(signing: &SigningKey, id: &str, wasm: &[u8]) -> Vec<u8> {
        let sha = hex(&Sha256::digest(wasm));
        let signature = signing.sign(&plugin_api::signing_payload(id, "1.0.0", &sha));
        plugin_api::seal(
            &Header {
                id: id.into(),
                version: "1.0.0".into(),
                name: "Shop tools".into(),
                sha256: sha,
                bytes: wasm.len() as u64,
                key: "mesafab-2026".into(),
                signature: hex(&signature.to_bytes()),
            },
            wasm,
        )
    }

    #[test]
    fn a_plugin_signed_by_a_trusted_key_is_taken() {
        let (signing, trusted) = publisher();
        let file = signed(&signing, "shop-tools", b"\0asm\x01\0\0\0");
        let checked = check(&trusted, &file).unwrap();
        assert_eq!(checked.header.id, "shop-tools");
        assert_eq!(checked.wasm, b"\0asm\x01\0\0\0");
        assert_eq!(checked.digest.len(), 64);
    }

    #[test]
    fn a_plugin_changed_after_signing_is_refused() {
        let (signing, trusted) = publisher();
        let mut file = signed(&signing, "shop-tools", b"\0asm\x01\0\0\0");
        let last = file.len() - 1;
        file[last] ^= 1;
        assert!(check(&trusted, &file).unwrap_err().contains("whole"));
    }

    #[test]
    fn a_plugin_signed_by_anybody_else_is_refused() {
        let (_, trusted) = publisher();
        let stranger = SigningKey::from_bytes(&[9u8; 32]);
        let file = signed(&stranger, "shop-tools", b"\0asm\x01\0\0\0");
        assert!(check(&trusted, &file).unwrap_err().contains("not signed"));
    }

    #[test]
    fn a_renamed_plugin_is_not_the_plugin_that_was_signed() {
        // The id is part of what is signed, so a signed plugin cannot be
        // passed off as a different one.
        let (signing, trusted) = publisher();
        let file = signed(&signing, "shop-tools", b"\0asm\x01\0\0\0");
        let text = String::from_utf8_lossy(&file).replace("shop-tools", "shop-tool2");
        assert!(check(&trusted, text.as_bytes()).is_err());
    }

    /// A plugin sealed by `publish.py sign-plugin` with the same fixed test
    /// key, so the script and the program cannot drift apart.
    #[test]
    fn a_plugin_sealed_by_the_publishing_script_is_taken() {
        let (_, trusted) = publisher();
        let file = include_bytes!("../tests/fixtures/sealed-by-publish-py.hvplugin");
        let checked = check(&trusted, file).unwrap();
        assert_eq!(checked.header.id, "script-test");
        assert_eq!(checked.wasm, b"\0asm\x01\0\0\0");
    }
}
