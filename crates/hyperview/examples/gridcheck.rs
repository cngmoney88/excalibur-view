//! Reads the grid off every sheet of a real drawing set and says what it found.
//!
//! Unit tests prove the rule; this proves the rule was the right one. A grid
//! reader that works on made-up marks and finds nothing on an actual stamped
//! structural set is a grid reader that does not work.
//!
//!     cargo run -p hyperview --example gridcheck -- "path/to/set.pdf"

fn main() {
    let path = match std::env::args().nth(1) {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            eprintln!("give it a drawing set");
            std::process::exit(2);
        }
    };
    let library = std::env::var("PDFIUM_DIR").ok().map(std::path::PathBuf::from);
    let count = match hyperview::render::pages_in(library.clone(), &path) {
        Ok(count) => count,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(1);
        }
    };
    println!("{count} sheets in {}", path.display());

    let how = takeoff::grid::HowClose::default();
    let mut with_a_grid = 0;
    for page in 0..count as u32 {
        match hyperview::render::grid_on(library.clone(), &path, page, how) {
            Ok(grid) if grid.is_empty() => {
                println!("  sheet {:>3}  —", page + 1);
            }
            Ok(grid) => {
                with_a_grid += 1;
                println!(
                    "  sheet {:>3}  {}   ({} vertical, {} horizontal)",
                    page + 1,
                    grid.extent(),
                    grid.vertical.len(),
                    grid.horizontal.len()
                );
            }
            Err(why) => println!("  sheet {:>3}  could not be read: {why}", page + 1),
        }
    }
    println!("\n{with_a_grid} of {count} sheets carry a grid this could read.");
}
