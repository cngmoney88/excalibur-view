//! The PDF object model.
//!
//! A PDF file is a graph of eight object types. Everything else in this crate —
//! the parser, the incremental writer, every annotation Hyperview reads or writes —
//! is expressed in terms of what is here.

use std::fmt;

/// A name object such as `/Subtype`. Stored without the leading slash, and
/// decoded: `/A#20B` is held as `A B`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name(pub Vec<u8>);

impl Name {
    pub fn new(text: &str) -> Name {
        Name(text.as_bytes().to_vec())
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("")
    }

    pub fn is(&self, text: &str) -> bool {
        self.0 == text.as_bytes()
    }
}

impl fmt::Debug for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{}", String::from_utf8_lossy(&self.0))
    }
}

impl From<&str> for Name {
    fn from(text: &str) -> Name {
        Name::new(text)
    }
}

/// How a string was written in the file. Kept so that a value read and written
/// back unchanged comes out the way it went in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StringKind {
    /// `(text)`
    Literal,
    /// `<48656C6C6F>`
    Hex,
}

/// An indirect reference: `12 0 R`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Ref {
    pub number: u32,
    pub generation: u16,
}

impl Ref {
    pub fn new(number: u32, generation: u16) -> Ref {
        Ref { number, generation }
    }
}

/// A dictionary. Entry order is preserved because a file that round-trips
/// byte-for-byte is far easier to trust than one that merely parses the same.
#[derive(Clone, Default, PartialEq)]
pub struct Dict(pub Vec<(Name, Object)>);

impl Dict {
    pub fn new() -> Dict {
        Dict(Vec::new())
    }

    pub fn get(&self, key: &str) -> Option<&Object> {
        self.0.iter().find(|(k, _)| k.is(key)).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Object> {
        self.0.iter_mut().find(|(k, _)| k.is(key)).map(|(_, v)| v)
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Adds or replaces an entry, keeping the position of an existing key.
    pub fn set(&mut self, key: impl Into<Name>, value: Object) {
        let key = key.into();
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Object> {
        let at = self.0.iter().position(|(k, _)| k.is(key))?;
        Some(self.0.remove(at).1)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Name, Object)> {
        self.0.iter()
    }
}

impl fmt::Debug for Dict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<<")?;
        for (k, v) in &self.0 {
            write!(f, " {k:?} {v:?}")?;
        }
        write!(f, " >>")
    }
}

/// A stream: a dictionary plus bytes. The bytes are held exactly as they were in
/// the file, still encoded; `crate::filters` decodes on demand.
#[derive(Clone, PartialEq)]
pub struct Stream {
    pub dict: Dict,
    pub data: Vec<u8>,
}

impl fmt::Debug for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} stream[{} bytes]", self.dict, self.data.len())
    }
}

#[derive(Clone, PartialEq)]
pub enum Object {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    String(Vec<u8>, StringKind),
    Name(Name),
    Array(Vec<Object>),
    Dict(Dict),
    Stream(Box<Stream>),
    Ref(Ref),
}

impl fmt::Debug for Object {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Object::Null => write!(f, "null"),
            Object::Bool(b) => write!(f, "{b}"),
            Object::Int(i) => write!(f, "{i}"),
            Object::Real(r) => write!(f, "{r}"),
            Object::String(s, StringKind::Hex) => {
                write!(f, "<{}>", s.iter().map(|b| format!("{b:02X}")).collect::<String>())
            }
            Object::String(s, _) => write!(f, "({})", String::from_utf8_lossy(s)),
            Object::Name(n) => write!(f, "{n:?}"),
            Object::Array(a) => {
                write!(f, "[")?;
                for o in a {
                    write!(f, " {o:?}")?;
                }
                write!(f, " ]")
            }
            Object::Dict(d) => write!(f, "{d:?}"),
            Object::Stream(s) => write!(f, "{s:?}"),
            Object::Ref(r) => write!(f, "{} {} R", r.number, r.generation),
        }
    }
}

impl Object {
    pub fn name(text: &str) -> Object {
        Object::Name(Name::new(text))
    }

    /// A literal string, encoded the way PDF wants text and escaped on the way
    /// out by the writer.
    pub fn text(text: &str) -> Object {
        Object::String(crate::text::encode(text), StringKind::Literal)
    }

    pub fn real(value: f64) -> Object {
        if value.fract() == 0.0 && value.abs() < 1e15 {
            Object::Int(value as i64)
        } else {
            Object::Real(value)
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Object::Int(i) => Some(*i),
            Object::Real(r) => Some(*r as i64),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Object::Int(i) => Some(*i as f64),
            Object::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Object::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_name(&self) -> Option<&Name> {
        match self {
            Object::Name(n) => Some(n),
            _ => None,
        }
    }

    /// True when this is the name `/wanted`.
    pub fn is_name(&self, wanted: &str) -> bool {
        matches!(self, Object::Name(n) if n.is(wanted))
    }

    /// A string of raw bytes, written literally.
    ///
    /// For the parts of a file that are bytes rather than words — a hash, a
    /// wrapped key — where running them through a text encoding would change
    /// them into something else.
    pub fn bytes(bytes: &[u8]) -> Object {
        Object::String(bytes.to_vec(), StringKind::Literal)
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.as_string()
    }

    pub fn as_string(&self) -> Option<&[u8]> {
        match self {
            Object::String(s, _) => Some(s),
            _ => None,
        }
    }

    /// A string read as text, honouring whichever encoding it was written in.
    pub fn as_text(&self) -> Option<String> {
        self.as_string().map(crate::text::decode)
    }

    pub fn as_array(&self) -> Option<&[Object]> {
        match self {
            Object::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The dictionary of either a dictionary object or a stream, which is what
    /// callers nearly always mean.
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Object::Dict(d) => Some(d),
            Object::Stream(s) => Some(&s.dict),
            _ => None,
        }
    }

    pub fn as_dict_mut(&mut self) -> Option<&mut Dict> {
        match self {
            Object::Dict(d) => Some(d),
            Object::Stream(s) => Some(&mut s.dict),
            _ => None,
        }
    }

    pub fn as_stream(&self) -> Option<&Stream> {
        match self {
            Object::Stream(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_ref(&self) -> Option<Ref> {
        match self {
            Object::Ref(r) => Some(*r),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Object::Null)
    }

    /// Reads a rectangle, normalised so the first corner is the lower left.
    /// Files in the wild routinely write them the other way round.
    pub fn as_rect(&self) -> Option<[f64; 4]> {
        let a = self.as_array()?;
        if a.len() < 4 {
            return None;
        }
        let v: Vec<f64> = a.iter().take(4).map(|o| o.as_f64().unwrap_or(0.0)).collect();
        Some([
            v[0].min(v[2]),
            v[1].min(v[3]),
            v[0].max(v[2]),
            v[1].max(v[3]),
        ])
    }

    pub fn numbers(&self) -> Vec<f64> {
        self.as_array()
            .map(|a| a.iter().filter_map(|o| o.as_f64()).collect())
            .unwrap_or_default()
    }
}

/// Builds a dictionary without ceremony: `dict!{"Type" => Object::name("Annot")}`.
#[macro_export]
macro_rules! dict {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut d = $crate::Dict::new();
        $( d.set($crate::Name::new($key), $value); )*
        d
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_a_key_twice_replaces_it_where_it_already_sat() {
        let mut d = Dict::new();
        d.set(Name::new("A"), Object::Int(1));
        d.set(Name::new("B"), Object::Int(2));
        d.set(Name::new("A"), Object::Int(9));
        assert_eq!(d.len(), 2);
        assert_eq!(d.get("A").unwrap().as_i64(), Some(9));
        assert!(d.0[0].0.is("A"), "the key kept its position");
    }

    #[test]
    fn a_rectangle_written_upside_down_still_reads_lower_left_first() {
        let r = Object::Array(vec![
            Object::Int(100),
            Object::Int(200),
            Object::Int(10),
            Object::Int(20),
        ]);
        assert_eq!(r.as_rect(), Some([10.0, 20.0, 100.0, 200.0]));
    }

    #[test]
    fn a_whole_real_is_written_as_an_integer() {
        assert_eq!(Object::real(4.0), Object::Int(4));
        assert_eq!(Object::real(4.5), Object::Real(4.5));
    }

    #[test]
    fn a_stream_answers_dictionary_questions() {
        let s = Object::Stream(Box::new(Stream {
            dict: dict! {"Type" => Object::name("ObjStm")},
            data: vec![],
        }));
        assert!(s.as_dict().unwrap().get("Type").unwrap().is_name("ObjStm"));
    }
}
