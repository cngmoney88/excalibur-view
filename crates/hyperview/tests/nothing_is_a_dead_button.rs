//! No button that does nothing.
//!
//! The instruction this program was built to was "everything Bluebeam has",
//! and the failure mode of that instruction is not a missing menu — it is a
//! menu that is complete and a third of it dead. A dead button is worse than
//! an absent one: somebody finds it, clicks it, nothing happens, and from
//! then on they do not trust the ones that do work.
//!
//! The menus already refuse to name a command that does not exist. This is
//! the other half of it: a command that exists must be *handled* somewhere.

use std::collections::BTreeSet;

/// Every .rs file in the viewer, read once.
fn the_whole_viewer() -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut all = String::new();
    let mut look = vec![root];
    while let Some(here) = look.pop() {
        let Ok(entries) = std::fs::read_dir(&here) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                look.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    all.push_str(&text);
                    all.push('\n');
                }
            }
        }
    }
    assert!(
        all.len() > 10_000,
        "the source could not be read, so this test proves nothing"
    );
    all
}

#[test]
fn every_command_on_a_menu_or_a_toolbar_is_handled_somewhere() {
    let source = the_whole_viewer();
    let mut dead: Vec<&str> = Vec::new();

    for command in ui::command::ALL {
        // A label or a gap is not something anybody can click.
        if matches!(
            command.kind,
            ui::command::Kind::Label | ui::command::Kind::Separator
        ) {
            continue;
        }
        // Handled means its identifier is written down somewhere in the
        // viewer: an arm of the command dispatch, a tool table, a panel that
        // answers for itself. What it must not be is nowhere at all, because
        // then the only thing that can happen when it is clicked is nothing.
        if !source.contains(&format!("\"{}\"", command.id)) {
            dead.push(command.id);
        }
    }

    assert!(
        dead.is_empty(),
        "{} command(s) are on the menus with nothing behind them: {:?}",
        dead.len(),
        dead
    );
}

#[test]
fn no_two_commands_share_an_identifier() {
    // Two entries with one id means one of them can never be reached, and
    // which one depends on the order of a list nobody is reading.
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for command in ui::command::ALL {
        if command.kind == ui::command::Kind::Separator {
            continue;
        }
        assert!(
            seen.insert(command.id),
            "two commands both call themselves {}",
            command.id
        );
    }
}

#[test]
fn no_two_commands_claim_the_same_keys() {
    // A shortcut that two things answer to is a shortcut that does the wrong
    // one of them, and the person pressing it never finds out why.
    let mut seen: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for command in ui::command::ALL {
        let Some(keys) = command.shortcut else {
            continue;
        };
        if let Some(already) = seen.insert(keys, command.id) {
            panic!("{already} and {} both answer to {keys}", command.id);
        }
    }
}

#[test]
fn the_three_new_ones_are_really_there() {
    // The features this release is for. Named explicitly, because "the tests
    // pass" is not the same as "the thing he asked for is in the build".
    let source = the_whole_viewer();
    for (id, handler) in [
        ("Document.ShopList", "fn shop_list"),
        ("Document.Doubles", "fn check_doubles"),
        ("Document.RevisionCost", "fn revision_cost"),
    ] {
        assert!(
            ui::command::find(id).is_some(),
            "{id} is not a command at all"
        );
        assert!(source.contains(handler), "{id} has no {handler}");
        assert!(
            source.contains(&format!("\"{id}\" => self.")),
            "{id} is not wired into the command dispatch"
        );
    }
}
