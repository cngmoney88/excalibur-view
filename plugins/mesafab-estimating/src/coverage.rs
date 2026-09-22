//! What the drawings name and the takeoff doesn't have.
//!
//! Two ways a whole piece of the job goes missing: a sheet full of member
//! callouts with nothing measured on it, and a size the drawings call for
//! that no markup anywhere is taken off as.

use std::collections::{BTreeMap, BTreeSet};

use plugin_api::{Finding, Input, Level, Output, Table};

use crate::callout;

pub fn check(input: &Input) -> Output {
    let mut findings = Vec::new();
    let mut rows = Vec::new();
    // Every size the takeoff has, however it was written.
    let taken: BTreeSet<String> = input
        .markups
        .iter()
        .filter_map(|m| callout::size_of(&m.subject).map(|s| s.designation))
        .collect();
    // Every size the drawings name: which sheets, and the first place.
    let mut named: BTreeMap<String, (Vec<u32>, [f64; 4], usize)> = BTreeMap::new();

    for sheet in &input.sheets {
        let mut sizes_here: BTreeMap<String, usize> = BTreeMap::new();
        for word in &sheet.words {
            for shape in callout::sizes_in(&word.text) {
                *sizes_here.entry(shape.designation.clone()).or_default() += 1;
                let entry = named.entry(shape.designation).or_insert((Vec::new(), word.area, 0));
                if !entry.0.contains(&sheet.page) {
                    entry.0.push(sheet.page);
                }
                entry.2 += 1;
            }
        }
        let measured = input
            .markups
            .iter()
            .filter(|m| m.page == sheet.page && m.kind != "markup")
            .count();
        let callouts: usize = sizes_here.values().sum();
        let verdict = if callouts >= 3 && measured == 0 {
            let mut top: Vec<(&String, &usize)> = sizes_here.iter().collect();
            top.sort_by(|a, b| b.1.cmp(a.1));
            let list = top.iter().take(4).map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(", ");
            findings.push(
                Finding::new(
                    Level::Check,
                    format!("{}: names {callouts} members ({list}…) and nothing on it is taken off.", sheet.name),
                )
                .on(sheet.page),
            );
            "nothing taken off"
        } else if measured > 0 {
            "taken off"
        } else {
            "—"
        };
        rows.push(vec![sheet.name.clone(), callouts.to_string(), measured.to_string(), verdict.to_string()]);
    }

    let sheet_name = |page: u32| {
        input
            .sheets
            .iter()
            .find(|s| s.page == page)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| format!("Sheet {}", page + 1))
    };
    for (size, (pages, first, times)) in &named {
        if taken.contains(size) {
            continue;
        }
        let where_ = pages.iter().take(5).map(|p| sheet_name(*p)).collect::<Vec<_>>().join(", ");
        findings.push(
            Finding::new(
                Level::Check,
                format!(
                    "{size} is called out {times} time{} on {where_}{} and nothing is taken off as {size}.",
                    if *times == 1 { "" } else { "s" },
                    if pages.len() > 5 { " and more" } else { "" }
                ),
            )
            .on(pages[0])
            .at(*first),
        );
    }
    findings.sort_by(|a, b| b.level.cmp(&a.level).then(a.page.cmp(&b.page)));
    Output {
        title: "Takeoff coverage".into(),
        summary: format!(
            "{} sizes named on {} sheets; {} of them are in the takeoff.",
            named.len(),
            input.sheets.len(),
            named.keys().filter(|k| taken.contains(*k)).count()
        ),
        findings,
        tables: vec![Table {
            title: "Every sheet".into(),
            columns: ["Sheet", "Sizes named", "Measurements", ""].iter().map(|s| s.to_string()).collect(),
            rows,
            totals: Vec::new(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_api::{Markup, Sheet, Word};

    #[test]
    fn a_size_nobody_took_off_and_a_sheet_nobody_touched_are_both_named() {
        let w = |t: &str| Word { text: t.into(), area: [0.0, 0.0, 10.0, 10.0], size: 8.0 };
        let input = Input {
            sheets: vec![
                Sheet { page: 0, name: "S-201".into(), words: vec![w("W12X26"), w("W12X26"), w("W8X10")], ..Default::default() },
                Sheet { page: 1, name: "S-202".into(), words: vec![w("W16X31"), w("W16X31"), w("L4X4X1/2")], ..Default::default() },
            ],
            markups: vec![Markup { id: "a".into(), page: 0, kind: "length".into(), subject: "W12x26".into(), quantity: 1.0, ..Default::default() }],
            ..Default::default()
        };
        let out = check(&input);
        let said: Vec<&str> = out.findings.iter().map(|f| f.message.as_str()).collect();
        assert!(said.iter().any(|m| m.starts_with("S-202: names 3 members")), "{said:?}");
        assert!(said.iter().any(|m| m.starts_with("W8X10 is called out")));
        assert!(!said.iter().any(|m| m.starts_with("W12X26")), "taken off, however it was written");
    }
}
