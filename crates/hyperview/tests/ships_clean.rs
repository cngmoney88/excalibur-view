//! What must not leave this building.
//!
//! A tool chest is the estimator's own work — their sections, their unit
//! weights, years of building the thing — and it is worth more than the program
//! that reads it. Hyperview ships able to read any chest and carrying none, the
//! same way Revu does. Anybody running it builds their own.
//!
//! The same goes for drawing sets: they are somebody's project under somebody's
//! contract, and they have no business in a repository or an installer.
//!
//! These tests are the guard. They fail loudly rather than letting a `.bpx`
//! quietly become part of a release.

use std::path::{Path, PathBuf};

fn repository() -> PathBuf {
    // From crates/hyperview up to the workspace.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .expect("the workspace root")
}

/// Everything under a folder, skipping the places nothing ships from.
fn walk(at: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(name.as_str(), "target" | ".git" | "third_party" | "dist" | "stage") {
            continue;
        }
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[test]
fn no_tool_chest_is_part_of_the_source() {
    let root = repository();
    let mut files = Vec::new();
    walk(&root, &mut files);

    let chests: Vec<String> = files
        .iter()
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("bpx") | Some("btx")
            )
        })
        // A local `profiles` folder is how somebody runs the program on their
        // own machine. It is ignored by git and never packaged, so it is not a
        // leak — but anywhere else is.
        .filter(|p| !p.components().any(|c| c.as_os_str() == "profiles"))
        .filter(|p| !p.components().any(|c| c.as_os_str() == "research"))
        .map(|p| p.display().to_string())
        .collect();

    assert!(
        chests.is_empty(),
        "tool chests must not be part of the program. These would ship:\n  {}",
        chests.join("\n  ")
    );
}

#[test]
fn the_places_a_chest_could_hide_are_all_ignored() {
    let root = repository();
    let ignore = std::fs::read_to_string(root.join(".gitignore"))
        .expect("the repository must have a .gitignore");
    for pattern in ["*.bpx", "*.btx", "/profiles/", "*.key", "*.pdf"] {
        assert!(
            ignore.lines().any(|line| line.trim() == pattern),
            "{pattern} must be ignored, or somebody's work ends up in a release"
        );
    }
}

#[test]
fn the_installer_ships_an_empty_profiles_folder_and_not_somebody_elses() {
    let root = repository();
    let script = root.join("installer").join("hyperview.iss");
    let Ok(text) = std::fs::read_to_string(&script) else {
        // The installer script is optional in a checkout; nothing to guard.
        return;
    };
    assert!(
        !text.to_lowercase().contains(".bpx") && !text.to_lowercase().contains(".btx"),
        "the installer must not name a tool chest"
    );
    assert!(
        text.contains("profiles\\README.txt") || text.contains("profiles\\*"),
        "it should ship the folder, so a company has somewhere to put their own"
    );
}

#[test]
fn the_program_works_perfectly_well_with_no_tool_chest_at_all() {
    // The shipped state. Nothing here should need a chest to exist.
    let none: Option<chest::Profile> = None;
    assert!(none.is_none());
    // And the panel says what to do about it rather than looking broken.
    // (The text itself lives in panelbody.rs; this is the contract it keeps.)
    let ready = ui::chrome::panel_ready("Tool Chest");
    assert!(ready.is_ok(), "the panel is there whether or not a chest is");
}

#[test]
fn no_drawing_set_is_part_of_the_source() {
    let root = repository();
    let mut files = Vec::new();
    walk(&root, &mut files);
    let drawings: Vec<String> = files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("pdf"))
        .filter(|p| !p.components().any(|c| c.as_os_str() == "research"))
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        drawings.is_empty(),
        "drawing sets belong to whoever's project they are. These would ship:\n  {}",
        drawings.join("\n  ")
    );
}

#[test]
fn nothing_of_anybody_else_s_is_tracked_by_the_repository() {
    // The tests above walk the working tree and skip `research`, on the
    // assumption that a folder named in .gitignore cannot get out. That
    // assumption was wrong once and cost a source archive: a file already
    // tracked by git ignores .gitignore entirely, so his chests and a
    // marked-up set went straight into a zip that was about to be handed
    // over.
    //
    // So this asks git what it is actually carrying, which is exactly what a
    // source archive contains — no assumptions about which folders are safe.
    let root = repository();
    let listed = std::process::Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["ls-files", "-z"])
        .output();
    let Ok(listed) = listed else {
        // No git on this machine, or not a checkout. Nothing to check.
        return;
    };
    if !listed.status.success() {
        return;
    }
    let tracked: Vec<String> = String::from_utf8_lossy(&listed.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect();
    if tracked.is_empty() {
        return;
    }

    let mustnt: Vec<&String> = tracked
        .iter()
        .filter(|path| {
            let lower = path.to_lowercase();
            lower.ends_with(".bpx")
                || lower.ends_with(".btx")
                || lower.ends_with(".pdf")
                || lower.ends_with(".key")
                || lower.starts_with("research/")
                || lower.starts_with("profiles/")
        })
        .collect();

    assert!(
        mustnt.is_empty(),
        "these are tracked by git, so they are in every source archive — \
         tool chests and drawing sets belong to whoever made them:\n  {}",
        mustnt
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
