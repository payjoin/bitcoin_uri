//! The ordered multimap of raw query parameters that backs [`crate::Uri`].
//!
//! Values are kept exactly as they appeared in the source URI - still
//! percent-encoded, still in their original key case - until something asks
//! for a decoded view. This is what lets re-emission be byte-exact and lets
//! every extension (payjoin's `pj`, a silent-payment `sp`, anything else)
//! read whatever it needs without this crate knowing what the value means.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// One `key[=value]` pair from a BIP 321 query string, in source order.
///
/// `otherparam = qchar *qchar [ "=" *qchar ]` (BIP 321): the `=value` part
/// is optional, so `raw_value` is `None` for a value-less parameter such as
/// a bare `?foo` rather than `?foo=`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamEntry {
    /// The parameter key, in the exact case it appeared in the source URI.
    pub key: String,
    /// The parameter value, still percent-encoded. `None` for a value-less
    /// parameter, `Some("")` for a parameter with an empty value (`?foo=`).
    pub raw_value: Option<String>,
}

pub(crate) fn requirement_key(key: &str) -> Option<&str> {
    let prefix = key.get(..4)?;
    if prefix.eq_ignore_ascii_case("req-") {
        Some(&key[4..])
    } else {
        None
    }
}

pub(crate) fn base_key(key: &str) -> &str {
    requirement_key(key).unwrap_or(key)
}

impl ParamEntry {
    /// `true` if `key` matches this entry's key ASCII-case-insensitively.
    ///
    /// BIP 321 keys are case-insensitive; this is the single place that
    /// comparison happens so every lookup agrees on it.
    pub(crate) fn key_eq(&self, key: &str) -> bool {
        self.key.eq_ignore_ascii_case(key)
    }

    /// `true` if `key` matches this entry's base key case-insensitively.
    ///
    /// A required key such as `req-pj` has base key `pj`; plain keys use the
    /// key itself. A full-key match is also accepted for callers that already
    /// include the prefix.
    pub(crate) fn base_key_eq(&self, key: &str) -> bool {
        self.key_eq(key) || self.requirement().is_some_and(|requirement| requirement.eq_ignore_ascii_case(key))
    }

    /// The requirement key if this is a `req-` parameter, otherwise `None`.
    ///
    /// BIP 321's `reqparam = "req-" otherparam`: a `req-` prefix (matched
    /// case-insensitively) marks a parameter a client MUST understand or
    /// reject the whole URI. This returns the key without that prefix -
    /// `req-pj` yields `Some("pj")` - which is what
    /// [`Uri::unsatisfied_requirements`](crate::Uri::unsatisfied_requirements)
    /// compares against the keys a client understands.
    pub fn requirement(&self) -> Option<&str> {
        requirement_key(&self.key)
    }

    /// `true` if this is a `req-` parameter (see [`requirement`]).
    ///
    /// [`requirement`]: ParamEntry::requirement
    pub fn is_required(&self) -> bool {
        self.requirement().is_some()
    }

    /// The value percent-decoded to a UTF-8 string, or `None` for a value-less
    /// parameter.
    ///
    /// Decoding is RFC 3986, so `+` stays a literal plus. `Some(Err(_))` when
    /// the value is not valid percent-encoding or not UTF-8 - the same
    /// access-time distinction the typed accessors
    /// ([`Uri::label`](crate::Uri::label) and friends) rely on, since they are
    /// built on this. An extension reading `pj`, `sp`, or another key gets the
    /// decoded text from here and only has to parse it.
    pub fn decoded_value(&self) -> Option<Result<String, crate::AccessError>> {
        self.raw_value.as_deref().map(crate::decode_to_string)
    }
}

/// An ordered multimap of [`ParamEntry`], preserving source order and
/// allowing repeats except where BIP 321 forbids them.
///
/// `amount`, `label`, `message`, and `pop` may not repeat (BIP 321); any
/// other key may, and every occurrence is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Params {
    entries: Vec<ParamEntry>,
}

/// Keys that BIP 321 forbids from repeating, matched case-insensitively.
pub(crate) const NON_REPEATABLE_KEYS: [&str; 4] = ["amount", "label", "message", "pop"];

impl Params {
    /// An empty parameter list.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Appends an entry, without any uniqueness check.
    ///
    /// Parsing and [`crate::UriBuilder`] enforce the BIP 321 non-repeat rule
    /// themselves before this is called.
    pub(crate) fn push(&mut self, entry: ParamEntry) {
        self.entries.push(entry);
    }

    /// The first entry whose base key matches `key` case-insensitively.
    pub fn get(&self, key: &str) -> Option<&ParamEntry> {
        self.entries.iter().find(|entry| entry.base_key_eq(key))
    }

    /// Every entry whose base key matches `key` case-insensitively, in source
    /// order.
    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a ParamEntry> + 'a {
        self.entries.iter().filter(move |entry| entry.base_key_eq(key))
    }

    /// All entries, in source order.
    pub fn iter(&self) -> impl Iterator<Item = &ParamEntry> {
        self.entries.iter()
    }

    /// `true` if there are no parameters at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of entries, counting every repeat.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl fmt::Display for Params {
    /// Re-emits every entry as `key=value` (or bare `key` for a value-less
    /// one), joined by `&`, in source order. Does not write a leading `?`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for entry in &self.entries {
            if first {
                first = false;
            } else {
                write!(f, "&")?;
            }
            write!(f, "{}", entry.key)?;
            if let Some(value) = &entry.raw_value {
                write!(f, "={}", value)?;
            }
        }
        Ok(())
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = &'a ParamEntry;
    type IntoIter = core::slice::Iter<'a, ParamEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str) -> ParamEntry {
        ParamEntry {
            key: key.into(),
            raw_value: None,
        }
    }

    #[test]
    fn requirement_strips_a_case_insensitive_req_prefix() {
        assert_eq!(entry("req-pj").requirement(), Some("pj"));
        assert_eq!(entry("REQ-pj").requirement(), Some("pj"));
        assert_eq!(entry("Req-Pop").requirement(), Some("Pop"));
    }

    #[test]
    fn requirement_is_none_for_a_plain_key() {
        assert_eq!(entry("pj").requirement(), None);
        assert_eq!(entry("label").requirement(), None);
        // A key that merely starts with "req" but not "req-".
        assert_eq!(entry("require").requirement(), None);
    }

    #[test]
    fn a_bare_req_prefix_has_an_empty_requirement() {
        assert_eq!(entry("req-").requirement(), Some(""));
    }

    #[test]
    fn a_short_or_non_ascii_key_is_not_a_requirement() {
        // Shorter than "req-".
        assert_eq!(entry("re").requirement(), None);
        // First four bytes decode to "réq", which is not "req-".
        assert_eq!(entry("réq-pj").requirement(), None);
    }

    #[test]
    fn is_required_reflects_the_req_prefix() {
        assert!(entry("req-pj").is_required());
        assert!(!entry("pj").is_required());
    }
}
