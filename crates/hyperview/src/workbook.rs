//! The markups list as an Excel workbook.
//!
//! Numbers go in as numbers — a length is 24.5, not the text "24'-6"" — so the
//! sheet can be added up, pivoted and pasted into a bid without anybody
//! retyping it. Two sheets: every markup, and what each group comes to. The
//! warning, when there is one, is the first thing on both.

use std::path::Path;

use rust_xlsxwriter::{Format, Workbook, XlsxError};
use takeoff::export::{GroupBy, Listing};

pub fn write(listing: &Listing, path: &Path) -> Result<(), String> {
    build(listing, path).map_err(|e| e.to_string())
}

fn build(listing: &Listing, path: &Path) -> Result<(), XlsxError> {
    let mut book = Workbook::new();
    let bold = Format::new().set_bold();
    let heading = Format::new().set_bold().set_background_color(0xE8EEF7);
    let warn = Format::new().set_bold().set_font_color(0x9C5700).set_background_color(0xFFF3CD);
    let two = Format::new().set_num_format("#,##0.00");
    let three = Format::new().set_num_format("#,##0.000");
    let whole = Format::new().set_num_format("#,##0");

    let grouped = listing.group != GroupBy::Nothing;

    // ---- every markup ----
    let sheet = book.add_worksheet();
    sheet.set_name("Markups")?;
    let mut r: u32 = 0;
    if let Some(warning) = listing.warning() {
        sheet.write_string_with_format(r, 0, format!("WARNING: {warning}"), &warn)?;
        r += 2;
    }
    let header_row = r;
    let mut c: u16 = 0;
    if grouped {
        sheet.write_string_with_format(r, c, listing.group.name(), &heading)?;
        sheet.set_column_width(c, 22)?;
        c += 1;
    }
    for column in &listing.columns {
        sheet.write_string_with_format(r, c, listing.heading(*column), &heading)?;
        sheet.set_column_width(c, width_of(*column))?;
        c += 1;
    }
    let last_col = c.saturating_sub(1);
    r += 1;
    for (group, lines) in listing.ordered() {
        for line in lines {
            let mut c: u16 = 0;
            if grouped {
                sheet.write_string(r, c, &group.key)?;
                c += 1;
            }
            for column in &listing.columns {
                match listing.number(line, *column) {
                    Some(n) => {
                        let format = match column {
                            takeoff::Column::Count => &whole,
                            takeoff::Column::Tons => &three,
                            _ => &two,
                        };
                        sheet.write_number_with_format(r, c, n, format)?;
                    }
                    None => {
                        let text = listing.cell(line, *column);
                        if !text.is_empty() {
                            sheet.write_string(r, c, text)?;
                        }
                    }
                }
                c += 1;
            }
            r += 1;
        }
    }
    if r > header_row + 1 {
        sheet.autofilter(header_row, 0, r - 1, last_col)?;
    }
    sheet.set_freeze_panes(header_row + 1, 0)?;

    // ---- what each group comes to ----
    let sheet = book.add_worksheet();
    sheet.set_name("Summary")?;
    let mut r: u32 = 0;
    sheet.write_string_with_format(r, 0, &listing.title, &bold)?;
    r += 1;
    sheet.write_string(r, 0, format!("Exported {} by {} from Excalibur View", listing.when, listing.who))?;
    r += 2;
    if let Some(warning) = listing.warning() {
        sheet.write_string_with_format(r, 0, format!("WARNING: {warning}"), &warn)?;
        r += 2;
    }
    let unit = listing.totals().unit;
    let headings = [
        if grouped { listing.group.name().to_string() } else { String::new() },
        "Markups".into(),
        "Count".into(),
        format!("Length{}", if unit.is_empty() { String::new() } else { format!(" ({unit})") }),
        format!("Area{}", if unit.is_empty() { String::new() } else { format!(" ({unit}²)") }),
        format!("Volume{}", if unit.is_empty() { String::new() } else { format!(" ({unit}³)") }),
        "Pounds".into(),
        "Tons".into(),
        "Left out (no scale)".into(),
    ];
    for (c, text) in headings.iter().enumerate() {
        sheet.write_string_with_format(r, c as u16, text, &heading)?;
        sheet.set_column_width(c as u16, if c == 0 { 28 } else { 14 })?;
    }
    r += 1;
    let mut groups = listing.groups();
    groups.push(listing.totals());
    let last = groups.len() - 1;
    for (i, t) in groups.iter().enumerate() {
        let emphasis = i == last;
        let text = |s: &str| (s.to_string(), emphasis);
        let (key, _) = text(&t.key);
        if emphasis {
            sheet.write_string_with_format(r, 0, key, &bold)?;
        } else {
            sheet.write_string(r, 0, key)?;
        }
        sheet.write_number_with_format(r, 1, t.markups as f64, &whole)?;
        if t.count > 0.0 {
            sheet.write_number_with_format(r, 2, t.count, &whole)?;
        }
        for (c, v) in [(3u16, t.length), (4, t.area), (5, t.volume)] {
            if v > 0.0 {
                sheet.write_number_with_format(r, c, v, &two)?;
            }
        }
        if t.weighed {
            sheet.write_number_with_format(r, 6, t.pounds, &whole)?;
            sheet.write_number_with_format(r, 7, t.tons(), &three)?;
        }
        if t.unscaled > 0 {
            sheet.write_number_with_format(r, 8, t.unscaled as f64, &whole)?;
        }
        r += 1;
    }
    book.save(path)?;
    Ok(())
}

fn width_of(column: takeoff::Column) -> f64 {
    use takeoff::Column;
    match column {
        Column::Subject | Column::Comments => 28.0,
        Column::Measurement | Column::Label => 18.0,
        Column::Author | Column::Date | Column::CreationDate => 16.0,
        _ => 12.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use takeoff::export::Line;
    use takeoff::user::UserColumn;
    use takeoff::{Column, Row, WeightColumns};

    #[test]
    fn a_workbook_is_written_and_opens_as_a_zip() {
        let mut row = Row::blank();
        row.kind = annot::Kind::Length;
        row.subject = "W12x26".into();
        row.length = Some(24.5);
        row.scaled = true;
        row.count = 1.0;
        row.columns[0] = "26".into();
        let listing = Listing {
            title: "S-101.pdf".into(),
            who: "Test".into(),
            when: "2026-09-21".into(),
            columns: vec![Column::Page, Column::Subject, Column::Length, Column::UserDefined(0), Column::Pounds],
            user: vec![UserColumn::plain(0, "LBS Per FT")],
            weights: WeightColumns::default(),
            lines: vec![Line { row: &row, sheet: "S-101".into(), drawing: "S-101.pdf".into() }],
            group: GroupBy::Subject,
        };
        let path = std::env::temp_dir().join(format!("hv-workbook-{}.xlsx", std::process::id()));
        write(&listing, &path).expect("written");
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], b"PK", "an xlsx is a zip");
        let _ = std::fs::remove_file(&path);
    }
}
