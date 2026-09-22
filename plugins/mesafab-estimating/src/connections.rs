//! Connections: member ends, and what they add.
//!
//! Every piece of steel has two ends and every end is a connection. The
//! count comes straight from the takeoff — pieces times quantity — and the
//! allowance is the shop's own number, set here and kept.

use std::collections::BTreeMap;

use plugin_api::{Finding, Input, Level, Output, Table};

#[derive(Default)]
struct Line {
    pieces: f64,
    feet: f64,
    pounds: f64,
    unweighed: f64,
}

pub fn count(input: &Input) -> Output {
    let allowance = input.number("allowance", 10.0).max(0.0) / 100.0;
    let per_end = input.number("per_end", 0.0).max(0.0);
    let bolts = input.number("bolts_per_end", 0.0).max(0.0);
    let mut by: BTreeMap<String, Line> = BTreeMap::new();
    for m in input.markups.iter().filter(|m| matches!(m.kind.as_str(), "length" | "polylength")) {
        let key = if m.subject.trim().is_empty() { "(no subject)".to_string() } else { m.subject.trim().to_string() };
        let line = by.entry(key).or_default();
        let q = m.quantity.max(0.0);
        line.pieces += q;
        line.feet += m.length.unwrap_or(0.0) * q;
        match m.pounds {
            Some(lb) => line.pounds += lb,
            None => line.unweighed += q,
        }
    }
    let mut findings = Vec::new();
    let mut rows = Vec::new();
    let (mut pieces, mut ends, mut feet, mut pounds, mut added, mut all_bolts) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for (subject, line) in &by {
        let e = line.pieces * 2.0;
        let extra = line.pounds * allowance + e * per_end;
        pieces += line.pieces;
        ends += e;
        feet += line.feet;
        pounds += line.pounds;
        added += extra;
        all_bolts += e * bolts;
        if line.unweighed > 0.0 {
            findings.push(Finding::new(
                Level::Check,
                format!(
                    "{subject}: {} piece{} with no unit weight — no weight, and nothing for their connections in the allowance.",
                    whole(line.unweighed),
                    if line.unweighed == 1.0 { "" } else { "s" }
                ),
            ));
        }
        rows.push(vec![
            subject.clone(),
            whole(line.pieces),
            whole(e),
            format!("{:.1}", line.feet),
            format!("{:.0}", line.pounds),
            format!("{:.0}", extra),
            if bolts > 0.0 { whole(e * bolts) } else { String::new() },
        ]);
    }
    Output {
        title: "Connections".into(),
        summary: format!(
            "{} member ends on {} pieces. An allowance of {}% of {:.2} tons{} comes to {:.0} lb ({:.2} tons).{}",
            whole(ends),
            whole(pieces),
            allowance * 100.0,
            pounds / 2000.0,
            if per_end > 0.0 { format!(", plus {per_end} lb an end") } else { String::new() },
            added,
            added / 2000.0,
            if bolts > 0.0 { format!(" {} bolts at {} an end.", whole(all_bolts), whole(bolts)) } else { String::new() }
        ),
        findings,
        tables: vec![Table {
            title: "By member".into(),
            columns: ["Member", "Pieces", "Ends", "Feet", "Member lb", "Connection lb", "Bolts"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            rows,
            totals: vec![
                "Total".into(),
                whole(pieces),
                whole(ends),
                format!("{feet:.1}"),
                format!("{pounds:.0}"),
                format!("{added:.0}"),
                if bolts > 0.0 { whole(all_bolts) } else { String::new() },
            ],
        }],
    }
}

fn whole(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plugin_api::Markup;

    #[test]
    fn six_beams_drawn_once_are_twelve_ends() {
        let beam = |id: &str, q: f64, lb: Option<f64>| Markup {
            id: id.into(),
            kind: "length".into(),
            subject: "W12X26".into(),
            length: Some(20.0),
            quantity: q,
            pounds: lb,
            ..Default::default()
        };
        let mut input = Input {
            markups: vec![beam("a", 6.0, Some(6.0 * 520.0)), beam("b", 1.0, None)],
            ..Default::default()
        };
        input.settings.insert("allowance".into(), serde_json::json!(10.0));
        input.settings.insert("bolts_per_end".into(), serde_json::json!(4.0));
        let out = count(&input);
        let t = &out.tables[0];
        assert_eq!(t.rows[0][1], "7");
        assert_eq!(t.rows[0][2], "14");
        assert_eq!(t.rows[0][5], "312", "10% of 3120 lb");
        assert_eq!(t.rows[0][6], "56");
        assert_eq!(out.findings.len(), 1, "the one with no weight is named");
    }
}
