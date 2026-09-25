//! Markups that go together.
//!
//! A Cloud+ is two markups: the cloud, and a callout whose leader starts on
//! the cloud and whose box says what the cloud is about. Each is an ordinary
//! markup of its kind, so any other program shows and edits them — the cloud
//! is a cloud and the callout is a callout. What makes them one thing here is
//! that the callout says whose it is: by the cloud's name while it is being
//! worked on, and in the file by the standard reply-to entry, `/IRT`, with
//! `/RT /Group`, which is how Acrobat and Revu say "these are one group".
//!
//! Picking up either picks up both, so moving the cloud moves its words with
//! it, and deleting it deletes them too.

use crate::sheet::Mark;

/// The entry a part keeps its group's name in, while it has no place in the
/// file to point at.
pub const PART_OF: &str = "XEVPartOf";

/// What a Cloud+ is called, in its subject.
pub const CLOUD_PLUS: &str = "Cloud+";

/// The markup a part belongs to, by the part's own entries. `None` for a
/// markup that is not part of anything.
fn head_named(marks: &[Mark], index: usize) -> Option<usize> {
    let mark = marks.get(index)?;
    let dict = &mark.markup.dict;
    let by_name = dict.get(PART_OF).and_then(|o| o.as_text()).filter(|n| !n.is_empty());
    if let Some(name) = by_name {
        if let Some(head) = marks
            .iter()
            .position(|m| !m.gone && m.markup.name() == name)
            .filter(|h| *h != index)
        {
            return Some(head);
        }
    }
    let grouped = dict
        .get("RT")
        .and_then(|o| o.as_name())
        .is_some_and(|n| n.as_str() == "Group");
    if grouped {
        if let Some(target) = dict.get("IRT").and_then(|o| o.as_ref()) {
            return marks
                .iter()
                .position(|m| !m.gone && m.reference == Some(target))
                .filter(|h| *h != index);
        }
    }
    None
}

/// Whether `part` belongs to `head`.
fn belongs(marks: &[Mark], part: usize, head: usize) -> bool {
    part != head && !marks[part].gone && head_named(marks, part) == Some(head)
}

/// The markup that stands for the group `index` is in: its head, or itself.
pub fn head_of(marks: &[Mark], index: usize) -> usize {
    head_named(marks, index).unwrap_or(index)
}

/// Every markup in the same group as `index`, itself included, head first.
pub fn group_of(marks: &[Mark], index: usize) -> Vec<usize> {
    if index >= marks.len() {
        return Vec::new();
    }
    let head = head_of(marks, index);
    let mut out = vec![head];
    let named = marks[head].markup.name();
    let placed = marks[head].reference;
    for (i, m) in marks.iter().enumerate() {
        if i == head || m.gone {
            continue;
        }
        // A quick look before the full one: most markups are nobody's part.
        let dict = &m.markup.dict;
        let maybe = (!named.is_empty()
            && dict.get(PART_OF).and_then(|o| o.as_text()).is_some_and(|n| n == named))
            || (placed.is_some() && dict.get("IRT").and_then(|o| o.as_ref()) == placed);
        if maybe && belongs(marks, i, head) {
            out.push(i);
        }
    }
    out
}

/// Whether some markups are all one group, or one markup on its own.
pub fn one_thing(marks: &[Mark], held: &[usize]) -> bool {
    match held {
        [] => false,
        [_] => true,
        [first, rest @ ..] => {
            let head = head_of(marks, *first);
            rest.iter().all(|i| head_of(marks, *i) == head)
        }
    }
}

/// The callout of a Cloud+ group, when `index` is in one.
pub fn words_of(marks: &[Mark], index: usize) -> Option<usize> {
    group_of(marks, index)
        .into_iter()
        .find(|i| marks[*i].markup.subtype() == annot::Subtype::FreeText)
}

/// Before a save: points each part at its head's place in the file, the way
/// other programs read a group. `placed` gives the place of each head saved
/// so far, by name.
pub fn point_at_head(mark: &mut Mark, placed: &std::collections::HashMap<String, pdf::Ref>) {
    let Some(name) = mark
        .markup
        .dict
        .get(PART_OF)
        .and_then(|o| o.as_text())
        .filter(|n| !n.is_empty())
    else {
        return;
    };
    if let Some(at) = placed.get(&name) {
        mark.markup.set("IRT", pdf::Object::Ref(*at));
        mark.markup.set("RT", pdf::Object::name("Group"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use annot::{Markup, Subtype};

    fn named(subtype: Subtype, name: &str) -> Mark {
        let mut m = Markup::new(subtype);
        m.set_box([0.0, 0.0, 10.0, 10.0]);
        m.set_name(name);
        Mark::new(0, m)
    }

    fn part_of(subtype: Subtype, name: &str, head: &str) -> Mark {
        let mut mark = named(subtype, name);
        mark.markup.set(PART_OF, pdf::Object::text(head));
        mark
    }

    #[test]
    fn a_cloud_and_its_words_are_one_group_whichever_is_picked() {
        let marks = vec![
            named(Subtype::Square, "other"),
            named(Subtype::Square, "cloud"),
            part_of(Subtype::FreeText, "words", "cloud"),
        ];
        assert_eq!(group_of(&marks, 1), vec![1, 2]);
        assert_eq!(group_of(&marks, 2), vec![1, 2]);
        assert_eq!(group_of(&marks, 0), vec![0]);
        assert_eq!(words_of(&marks, 1), Some(2));
        assert!(one_thing(&marks, &[1, 2]));
        assert!(!one_thing(&marks, &[0, 2]));
    }

    #[test]
    fn a_group_from_another_program_is_read_by_reply_to() {
        let mut cloud = named(Subtype::Square, "");
        cloud.reference = Some(pdf::Ref::new(40, 0));
        let mut words = named(Subtype::FreeText, "");
        words.markup.set("IRT", pdf::Object::Ref(pdf::Ref::new(40, 0)));
        words.markup.set("RT", pdf::Object::name("Group"));
        // A reply that is only a reply is not a group.
        let mut reply = named(Subtype::Text, "");
        reply.markup.set("IRT", pdf::Object::Ref(pdf::Ref::new(40, 0)));
        let marks = vec![cloud, words, reply];
        assert_eq!(group_of(&marks, 0), vec![0, 1]);
        assert_eq!(group_of(&marks, 2), vec![2]);
    }

    #[test]
    fn a_part_whose_head_is_gone_is_on_its_own() {
        let mut marks = vec![named(Subtype::Square, "cloud"), part_of(Subtype::FreeText, "words", "cloud")];
        marks[0].gone = true;
        assert_eq!(group_of(&marks, 1), vec![1]);
    }

    #[test]
    fn saving_points_the_part_at_its_head() {
        let mut words = part_of(Subtype::FreeText, "words", "cloud");
        let mut placed = std::collections::HashMap::new();
        placed.insert("cloud".to_string(), pdf::Ref::new(12, 0));
        point_at_head(&mut words, &placed);
        assert_eq!(words.markup.dict.get("IRT").and_then(|o| o.as_ref()), Some(pdf::Ref::new(12, 0)));
        assert_eq!(words.markup.dict.get("RT").and_then(|o| o.as_name()).map(|n| n.as_str().to_string()), Some("Group".into()));
    }
}
