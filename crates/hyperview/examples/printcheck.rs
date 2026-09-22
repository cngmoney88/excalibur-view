//! `printcheck`: can this machine print from Hyperview? Lists the printers and
//! their paper sizes, and checks the PDF engine carried inside the program
//! has the call that draws onto a printer. Changes nothing, prints nothing.

fn main() {
    let library = hyperview::install::pdfium_library();
    println!("pdf engine: {}", library.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "(not carried)".into()));
    let draws = hyperview::winprint::render_to_dc(library.as_deref());
    println!("draws onto a printer: {}", if draws.is_some() { "yes" } else { "NO" });
    let (printers, default) = hyperview::winprint::printers();
    println!("printers: {}", printers.len());
    for p in &printers {
        let papers = hyperview::winprint::paper_sizes(p);
        let settings = hyperview::winprint::settings(p);
        println!(
            "  {p}{}  — {} paper sizes, settings {} bytes",
            if Some(p) == default.as_ref() { " (default)" } else { "" },
            papers.len(),
            settings.map(|s| s.len()).unwrap_or(0)
        );
    }
}
