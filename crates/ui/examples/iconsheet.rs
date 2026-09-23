//! Writes every icon out as one page, so the set can be looked at together.
//! Icons that do not look like siblings are easier to spot side by side than
//! one at a time on a toolbar.

fn main() {
    let all: &[(&str, ui::icon::Glyph)] = ui::icon::CONTACT_SHEET;
    let across = 10;
    let cell = 76.0;
    let pad = 12.0;
    let down = all.len().div_ceil(across);
    let w = across as f32 * cell;
    let h = down as f32 * (cell + 16.0);

    let mut svg = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{w}' height='{h}' viewBox='0 0 {w} {h}'>\
         <rect width='100%' height='100%' fill='#2b2b2b'/>"
    );
    for (i, (name, glyph)) in all.iter().enumerate() {
        let col = (i % across) as f32;
        let row = (i / across) as f32;
        let x = col * cell + pad;
        let y = row * (cell + 16.0) + pad;
        let scale = (cell - pad * 2.0) / 100.0;
        svg.push_str(&format!("<g transform='translate({x},{y}) scale({scale})'>"));
        for (d, filled) in *glyph {
            if *filled {
                svg.push_str(&format!("<path d='{d}' fill='#e8e8e8'/>"));
            } else {
                svg.push_str(&format!(
                    "<path d='{d}' fill='none' stroke='#e8e8e8' stroke-width='7' \
                     stroke-linecap='round' stroke-linejoin='round'/>"
                ));
            }
        }
        svg.push_str("</g>");
        svg.push_str(&format!(
            "<text x='{}' y='{}' fill='#999' font-family='sans-serif' font-size='9' \
             text-anchor='middle'>{name}</text>",
            x + (cell - pad * 2.0) / 2.0,
            y + cell - pad + 4.0
        ));
    }
    svg.push_str("</svg>");
    std::fs::write("/tmp/icons.svg", svg).unwrap();
    println!("{} icons written to /tmp/icons.svg", all.len());
}
