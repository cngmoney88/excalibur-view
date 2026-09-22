//! Form fields: boxes somebody fills in rather than draws.
//!
//! A shop fills in more forms than it draws: transmittals, RFIs, weld
//! procedure sheets, inspection reports, the shipping list. Revu's form tools
//! are what people use for that, so they are here.
//!
//! A form field in a PDF is two things at once: an *annotation* on a page, so
//! it can be seen, and a *field* in the document's own list, so it can be
//! filled in and its value read back. Hyperview writes both, which is the
//! difference between a box that looks like a field and one that is.

use crate::app::Tool;

/// What kind of box it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Text,
    CheckBox,
    RadioButton,
    ListBox,
    ComboBox,
    Button,
    Signature,
}

impl Field {
    pub const ALL: &'static [Field] = &[
        Field::Text,
        Field::CheckBox,
        Field::RadioButton,
        Field::ListBox,
        Field::ComboBox,
        Field::Button,
        Field::Signature,
    ];

    pub fn of_tool(tool: Tool) -> Option<Field> {
        Some(match tool {
            Tool::FormText => Field::Text,
            Tool::FormCheckBox => Field::CheckBox,
            Tool::FormRadio => Field::RadioButton,
            Tool::FormList => Field::ListBox,
            Tool::FormCombo => Field::ComboBox,
            Tool::FormButton => Field::Button,
            Tool::FormSignature => Field::Signature,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Field::Text => "Text field",
            Field::CheckBox => "Check box",
            Field::RadioButton => "Radio button",
            Field::ListBox => "List box",
            Field::ComboBox => "Combo box",
            Field::Button => "Button",
            Field::Signature => "Signature",
        }
    }

    /// What the format calls this kind of field.
    pub fn kind(self) -> &'static str {
        match self {
            Field::Text => "Tx",
            Field::CheckBox | Field::RadioButton | Field::Button => "Btn",
            Field::ListBox | Field::ComboBox => "Ch",
            Field::Signature => "Sig",
        }
    }

    /// The flags that tell one `Btn` or `Ch` from another.
    ///
    /// The format packs the difference between a check box, a radio button and
    /// a push button into bits of one number, which is why a field written
    /// with the wrong ones looks right and behaves like something else.
    pub fn flags(self) -> i64 {
        match self {
            // Bit 16 (1<<15): a push button, which holds no value.
            Field::Button => 1 << 16,
            // Bit 15 (1<<14): radio. Bit 16 must be off.
            Field::RadioButton => 1 << 15,
            Field::CheckBox => 0,
            // Bit 18 (1<<17): editable, which is what makes a combo a combo.
            Field::ComboBox => 1 << 17,
            Field::ListBox => 0,
            Field::Text | Field::Signature => 0,
        }
    }

    /// A short prefix for the name a new field gets.
    pub fn prefix(self) -> &'static str {
        match self {
            Field::Text => "Text",
            Field::CheckBox => "Check",
            Field::RadioButton => "Choice",
            Field::ListBox => "List",
            Field::ComboBox => "Combo",
            Field::Button => "Button",
            Field::Signature => "Signature",
        }
    }

    /// Whether the value of this field is something somebody types.
    pub fn is_typed(self) -> bool {
        matches!(self, Field::Text | Field::ComboBox)
    }
}

/// A name nothing else on the drawing is using.
///
/// Two fields with the same name are, in a PDF, the *same field* shown twice:
/// type in one and the other changes. That is occasionally what somebody
/// wants and never what they want by accident, so a new field always gets a
/// name of its own.
pub fn free_name(field: Field, taken: &std::collections::BTreeSet<String>) -> String {
    let prefix = field.prefix();
    for n in 1..100_000 {
        let name = format!("{prefix}{n}");
        if !taken.contains(&name) {
            return name;
        }
    }
    format!("{prefix}{}", taken.len() + 1)
}

/// Builds the annotation for a new field.
pub fn make(field: Field, name: &str, area: [f64; 4], look: &crate::pen::Pen) -> annot::Markup {
    let mut markup = annot::Markup::new(annot::Subtype::Other);
    markup.set("Subtype", pdf::Object::name("Widget"));
    markup.set("FT", pdf::Object::name(field.kind()));
    markup.set("T", pdf::Object::text(name));
    markup.set("Ff", pdf::Object::Int(field.flags()));
    markup.set_box(area);
    // Print it, like every other markup.
    markup.set("F", pdf::Object::Int(4));

    // How it looks: a border and a background, which is what /MK is for.
    let to_f = |v: u8| v as f64 / 255.0;
    let mut look_dict = pdf::Dict::new();
    look_dict.set(
        "BC",
        pdf::Object::Array(
            [look.line[0], look.line[1], look.line[2]]
                .iter()
                .map(|v| pdf::Object::real(to_f(*v)))
                .collect(),
        ),
    );
    if let Some(fill) = look.fill {
        look_dict.set(
            "BG",
            pdf::Object::Array(
                [fill[0], fill[1], fill[2]]
                    .iter()
                    .map(|v| pdf::Object::real(to_f(*v)))
                    .collect(),
            ),
        );
    }
    if field == Field::Button {
        look_dict.set("CA", pdf::Object::text(name));
    }
    markup.set("MK", pdf::Object::Dict(look_dict));

    // The colour on the markup itself as well as in /MK, because that is what
    // Hyperview draws the appearance from and /MK is what a reader uses when
    // it builds its own.
    markup.set_colour([
        to_f(look.line[0]) as f32,
        to_f(look.line[1]) as f32,
        to_f(look.line[2]) as f32,
    ]);
    if let Some(fill) = look.fill {
        markup.dict.set(
            "IC",
            pdf::Object::Array(
                [fill[0], fill[1], fill[2]]
                    .iter()
                    .map(|v| pdf::Object::real(to_f(*v)))
                    .collect(),
            ),
        );
    }
    markup.set_width(look.width.max(1.0));

    let mut border = pdf::Dict::new();
    border.set("W", pdf::Object::real(look.width.max(1.0)));
    border.set("S", pdf::Object::name("S"));
    markup.set("BS", pdf::Object::Dict(border));

    look.text.onto(&mut markup);

    match field {
        Field::CheckBox | Field::RadioButton => {
            // Off to start with, and "Yes" when it is on — which is what
            // every other program writes and what a reader expects to find.
            markup.set("V", pdf::Object::name("Off"));
            markup.set("AS", pdf::Object::name("Off"));
            markup.set("DV", pdf::Object::name("Off"));
        }
        Field::ListBox | Field::ComboBox => {
            markup.set("Opt", pdf::Object::Array(Vec::new()));
            markup.set("V", pdf::Object::text(""));
        }
        Field::Text => {
            markup.set("V", pdf::Object::text(""));
        }
        _ => {}
    }
    markup
}

/// Every field name already on a drawing.
pub fn names_taken(file: &pdf::Document) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for page in 0..file.page_count() {
        let Some(dict) = file.page(page) else { continue };
        for reference in file.annotations(&dict) {
            let object = file.get(reference);
            let Some(dict) = object.as_dict() else { continue };
            if !dict
                .get("Subtype")
                .and_then(|o| o.as_name())
                .map(|n| n.as_str() == "Widget")
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(name) = dict.get("T").and_then(|o| o.as_text()) {
                out.insert(name);
            }
        }
    }
    out
}

/// Whether a markup is a form field rather than something drawn.
pub fn is_field(markup: &annot::Markup) -> bool {
    markup
        .dict
        .get("Subtype")
        .and_then(|o| o.as_name())
        .map(|n| n.as_str() == "Widget")
        .unwrap_or(false)
}

/// What kind of field a markup is, read back from the file.
pub fn field_of(markup: &annot::Markup) -> Option<Field> {
    if !is_field(markup) {
        return None;
    }
    let kind = markup.dict.get("FT").and_then(|o| o.as_name())?;
    let flags = markup.dict.get("Ff").and_then(|o| o.as_i64()).unwrap_or(0);
    Some(match kind.as_str() {
        "Tx" => Field::Text,
        "Sig" => Field::Signature,
        "Btn" => {
            if flags & (1 << 16) != 0 {
                Field::Button
            } else if flags & (1 << 15) != 0 {
                Field::RadioButton
            } else {
                Field::CheckBox
            }
        }
        "Ch" => {
            if flags & (1 << 17) != 0 {
                Field::ComboBox
            } else {
                Field::ListBox
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_kind_of_field_reads_back_as_itself() {
        // The format packs the difference between a check box, a radio button
        // and a push button into bits of one number. A field written with the
        // wrong ones looks right and behaves like something else.
        for field in Field::ALL {
            let made = make(*field, "Thing1", [0.0, 0.0, 100.0, 20.0], &crate::pen::Pen::default());
            assert_eq!(field_of(&made), Some(*field), "{}", field.name());
        }
    }

    #[test]
    fn a_new_field_never_takes_a_name_already_in_use() {
        // Two fields with the same name are, in a PDF, the same field shown
        // twice: type in one and the other changes.
        let mut taken: BTreeSet<String> = BTreeSet::new();
        taken.insert("Text1".into());
        taken.insert("Text2".into());
        assert_eq!(free_name(Field::Text, &taken), "Text3");
        assert_eq!(free_name(Field::CheckBox, &taken), "Check1");
    }

    #[test]
    fn a_field_is_not_mistaken_for_something_somebody_drew() {
        let drawn = annot::Markup::new(annot::Subtype::Square);
        assert!(!is_field(&drawn));
        assert!(field_of(&drawn).is_none());
        let field = make(Field::Text, "Text1", [0.0, 0.0, 10.0, 10.0], &crate::pen::Pen::default());
        assert!(is_field(&field));
    }

    #[test]
    fn a_check_box_starts_off_rather_than_undefined() {
        // A check box with no value is one that shows differently in every
        // reader, which on an inspection sheet is a real problem.
        let made = make(
            Field::CheckBox,
            "Check1",
            [0.0, 0.0, 12.0, 12.0],
            &crate::pen::Pen::default(),
        );
        assert_eq!(
            made.dict.get("V").and_then(|o| o.as_name()).map(|n| n.as_str().to_string()),
            Some("Off".to_string())
        );
        assert!(made.dict.has("AS"), "and it shows that state");
    }

    #[test]
    fn a_push_button_holds_no_value() {
        let made = make(Field::Button, "Go", [0.0, 0.0, 60.0, 20.0], &crate::pen::Pen::default());
        assert!(!made.dict.has("V"));
        // And it wears its name, because a button with nothing written on it
        // is a button nobody presses.
        let mk = made.dict.get("MK").and_then(|o| o.as_dict()).expect("a look");
        assert_eq!(mk.get("CA").and_then(|o| o.as_text()), Some("Go".to_string()));
    }
}
