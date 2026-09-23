//! Puts the Excalibur mark, and what this program is, into the exe itself.
//!
//! Without this the program draws its own icon perfectly at run time and
//! Windows still shows a blank sheet of paper in Explorer, on the taskbar
//! before the window opens, and in the "open with" list. Those come from a
//! resource compiled into the binary, not from anything the program does once
//! it is running.
//!
//! The icon is generated from the same vector the program draws — see
//! `cargo run -p ui --example makeicon` — so there is one mark, not two that
//! can drift apart.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(embedded_pdfium)");

    let target = std::env::var("TARGET").unwrap_or_default();

    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .parent()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();

    // The PDF engine goes inside the program, so a company is handed one file
    // rather than a program and a library that has to be kept beside it. It is
    // written out the first time a drawing is opened.
    //
    // This happens for every platform, and before the Windows-only part below
    // returns. It used to sit after that return, which meant a Mac build
    // carried no engine at all and opened to "The PDF engine is missing" --
    // a program that looks finished and cannot do the one thing it is for.
    //
    // The name comes from TARGET, not from `cfg!`. In a build script `cfg!`
    // describes the machine doing the building, so a Mac cross-building for
    // Windows would go looking for a .dylib.
    let engine = if target.contains("windows") {
        "pdfium.dll"
    } else if target.contains("apple") || target.contains("darwin") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };
    let pdfium = root.join("third_party").join("pdfium").join("embedded").join(engine);
    println!("cargo:rerun-if-changed={}", pdfium.display());
    if pdfium.exists() {
        println!("cargo:rustc-cfg=embedded_pdfium");
        println!("cargo:rustc-env=HYPERVIEW_PDFIUM={}", pdfium.display());
    } else {
        println!("cargo:warning=third_party/pdfium/embedded/{engine} is missing, so the program will need {engine} beside it");
    }

    // The rest is the icon and version block Windows reads off the file
    // itself, and only Windows has either.
    if !target.contains("windows") {
        return;
    }

    let icon = root.join("assets").join("hyperview.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    if !icon.exists() {
        // Not a failure. A checkout without the generated icon should still
        // build a working program; it just gets Windows' blank page until
        // somebody runs the generator.
        println!("cargo:warning=assets/hyperview.ico is missing, so the exe will have no icon");
        return;
    }

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let script = out.join("hyperview.rc");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let (a, b, c) = split(&version);

    // The version block is what Windows shows on the Details tab of the file's
    // properties, and what an administrator looks at when six machines are
    // supposed to be on the same build and one of them is not.
    let rc = format!(
        r#"#include <winver.h>
1 ICON "{icon}"

VS_VERSION_INFO VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEOS VOS__WINDOWS32
FILETYPE VFT_APP
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Excalibur Construction Technologies"
            VALUE "FileDescription", "Excalibur View \0"
            VALUE "FileVersion", "{version}"
            VALUE "InternalName", "hyperview"
            VALUE "OriginalFilename", "Hyperview.exe"
            VALUE "ProductName", "Excalibur View"
            VALUE "ProductVersion", "{version}"
            VALUE "LegalCopyright", "Excalibur Construction Technologies"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#,
        icon = icon.display().to_string().replace('\\', "\\\\"),
    );
    std::fs::write(&script, rc).expect("could not write the resource script");

    let object = out.join("hyperview-resource.o");
    if !compile(&script, &object, &target) {
        println!("cargo:warning=no Windows resource compiler found, so the exe will have no icon");
        return;
    }
    println!("cargo:rustc-link-arg-bins={}", object.display());
}

/// Tries the resource compilers that might be on this machine: the one MSVC
/// builds use, then the mingw ones a cross-build from Linux has.
fn compile(script: &Path, object: &Path, target: &str) -> bool {
    let candidates: &[&str] = if target.contains("msvc") {
        &["rc.exe", "llvm-rc"]
    } else {
        &[
            "x86_64-w64-mingw32-windres",
            "x86_64-w64-mingw32ucrt-windres",
            "windres",
            "llvm-windres",
        ]
    };
    for tool in candidates {
        let run = if tool.ends_with("rc.exe") || *tool == "llvm-rc" {
            std::process::Command::new(tool)
                .arg(format!("/fo{}", object.display()))
                .arg(script)
                .status()
        } else {
            std::process::Command::new(tool)
                .args(["-I", script.parent().unwrap_or(Path::new(".")).to_str().unwrap_or(".")])
                .arg(script)
                .arg("-O")
                .arg("coff")
                .arg("-o")
                .arg(object)
                .status()
        };
        if matches!(run, Ok(status) if status.success()) {
            return true;
        }
    }
    false
}

/// "1.4.2" as three numbers, for the fields Windows wants as numbers.
fn split(version: &str) -> (u16, u16, u16) {
    let mut parts = version.split('.').map(|p| p.parse::<u16>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}
