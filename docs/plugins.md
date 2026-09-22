# Plugins

An office's own tools, in its own seats and nobody else's.

## What a plugin is

A WebAssembly module with three exports, sealed into a `.hvplugin` file with
the publisher's signature. The contract — every type a plugin is handed and
hands back — is `crates/plugin-api`. In Rust a plugin is two functions and a
macro:

```rust
plugin_api::plugin!(manifest, run);

fn manifest() -> plugin_api::Manifest { /* id, name, version, commands, settings */ }
fn run(input: plugin_api::Input) -> Result<plugin_api::Output, String> { /* ... */ }
```

`plugins/mesafab-estimating` is a whole one.

### What it is handed

For each sheet in the command's scope (the sheet on screen, or every sheet):
its name, size, the scale set on it, and — when the command asks for them —
the words printed on it and its straight line-work with each line's pen
width, all in sheet space (points, from the top left of the sheet as shown).
Then every markup on those sheets: kind, subject, label, points, length in
feet up any slope, area, count, quantity, custom columns and pounds. And the
command's settings, as the person running it left them.

### What it hands back

A title, a summary, findings and tables. A finding has a level (note, check,
problem), a sheet and an area to outline, and fixes. A fix is a label and a
list of actions from a short, fixed list:

| Action | |
|---|---|
| `set_sheet_scale` | a sheet's scale, by ratio (48 for 1/4" = 1'-0") |
| `set_subject` | a markup's subject |
| `set_column` | one of a markup's six custom columns |
| `set_quantity` | how many pieces a markup stands for |
| `set_slope` | a pitch, rise over run |
| `add_length` | a new length along given points, made with the office's tool of that subject when the chest has one |

Hyperview carries a fix out only when somebody clicks it, as one undo step.

### What it cannot do

Import anything. A module with any import is refused, so it has no file
system, no network, no clock and no way to reach the program. It gets
sixty billion instructions' worth of work and a gigabyte and a half of
memory per run, and is thrown away after each.

## Building and signing

```sh
plugins/mesafab-estimating/build.sh          # -> target/mesafab_estimating.wasm
```

On the publisher's PC, beside the signing key:

```powershell
py -3 publish.py sign-plugin --key hyperview-signing.key --id mesafab-estimating `
      --version 1.0.0 --name "Mesa Fab Estimating" mesafab_estimating.wasm
```

That writes `mesafab-estimating-1.0.0.hvplugin`. An administrator adds it
with **Plugins → Add Plugin…**; it goes to the office server, which checks
the signature and hands it to every seat within a few minutes. A newer
version with the same id replaces the older.

## Handing it out

The office server keeps plugins in its database and gives them to its own
seats only (`/api/v1/plugins`). **Plugins → Manage Plugins…** shows what a
seat has, where it came from and who signed it; an administrator can take
one off the office's list, and each seat drops it the next time it looks.
