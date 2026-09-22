//! Mesa Fab Estimating: the checks Mesa Fab runs on a takeoff before a
//! number goes out. A Hyperview plugin — see `plugin_api`.

pub mod callout;
pub mod connections;
pub mod coverage;
pub mod geo;
pub mod missed;
pub mod scales;
pub mod sizes;

use plugin_api::{Command, Input, Manifest, Needs, Output, Scope, Setting, SettingKind};

pub fn manifest() -> Manifest {
    let command = |id: &str, name: &str, description: &str, scope: Scope, words: bool, lines: bool| Command {
        id: id.into(),
        name: name.into(),
        description: description.into(),
        scope,
        needs: Needs { words, lines },
    };
    let number = |id: &str, name: &str, help: &str, default: f64, min: f64, max: f64, unit: &str| Setting {
        id: id.into(),
        name: name.into(),
        help: help.into(),
        kind: SettingKind::Number { default, min: Some(min), max: Some(max), unit: unit.into() },
    };
    Manifest {
        id: "mesafab-estimating".into(),
        name: "Mesa Fab Estimating".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        publisher: "Mesa Fab".into(),
        description: "Scale check, sizes read off the drawing, missed members, takeoff coverage and connections.".into(),
        abi: plugin_api::ABI,
        commands: vec![
            command(
                "scales",
                "Check the Scales",
                "Every sheet's scale against the scales printed on it: wrong scales, half-size prints, sheets with no scale, and lengths measured in details drawn at another scale.",
                Scope::Drawing,
                true,
                false,
            ),
            command(
                "sizes",
                "Read Sizes Off the Drawing",
                "Each length against the size written beside it, its unit weight against its size, TYP. notes counted once, and pitches measured flat.",
                Scope::Drawing,
                true,
                false,
            ),
            command(
                "missed",
                "Find Missed Members (This Sheet)",
                "Heavy lines on the sheet on screen that no length covers, with the size beside each and one click to take it off.",
                Scope::Sheet,
                true,
                true,
            ),
            command(
                "coverage",
                "Takeoff Coverage",
                "Sheets that name members and have nothing taken off, and sizes the drawings call for that aren't in the takeoff.",
                Scope::Drawing,
                true,
                false,
            ),
            command(
                "connections",
                "Count Connections",
                "Member ends from the takeoff, pieces times quantity, with the connection allowance and bolts.",
                Scope::Drawing,
                false,
                false,
            ),
        ],
        settings: vec![
            number("allowance", "Connection allowance", "Connection material, as a share of member weight.", 10.0, 0.0, 50.0, "%"),
            number("per_end", "Connection weight per end", "Added for every member end, on top of the allowance.", 0.0, 0.0, 500.0, "lb"),
            number("bolts_per_end", "Bolts per end", "Zero leaves bolts out.", 0.0, 0.0, 40.0, ""),
            number("heavier", "Member lines are heavier by", "How much heavier than the sheet's usual line a member is drawn.", 1.6, 1.1, 5.0, "×"),
            number("shortest", "Shortest member", "Heavy lines shorter than this aren't reported as missed.", 3.0, 0.0, 50.0, "ft"),
            Setting {
                id: "unsized".into(),
                name: "List lines with no size".into(),
                help: "Missed members lists heavy lines with a size written beside them. Turn this on to list the rest too — walls, outlines and edges, mostly.".into(),
                kind: SettingKind::Toggle { default: false },
            },
            Setting {
                id: "dashed".into(),
                name: "Dashed lines can be members".into(),
                help: "Joists and hidden members are often drawn dashed.".into(),
                kind: SettingKind::Toggle { default: false },
            },
        ],
    }
}

pub fn run(input: Input) -> Result<Output, String> {
    match input.command.as_str() {
        "scales" => Ok(scales::check(&input)),
        "sizes" => Ok(sizes::check(&input)),
        "missed" => Ok(missed::check(&input)),
        "coverage" => Ok(coverage::check(&input)),
        "connections" => Ok(connections::count(&input)),
        other => Err(format!("Mesa Fab Estimating has no command called {other}.")),
    }
}

plugin_api::plugin!(manifest, run);

#[cfg(test)]
mod tests {
    #[test]
    fn every_command_in_the_manifest_runs() {
        for command in super::manifest().commands {
            let input = plugin_api::Input { command: command.id.clone(), ..Default::default() };
            assert!(super::run(input).is_ok(), "{}", command.id);
        }
    }
}
