//! Rust-idiomatic, compliant, flexible and performant bitcoin URI crate.
//!
//! * Rust-idiomatic: owned types, no lifetime parameter.
//! * Compliant: parses the BIP 321 shape.
//! * Flexible: an extension reads whatever params it needs straight off the
//!   parsed [`Uri`]; this crate never needs to know what they mean.
//! * Performant: values stay percent-encoded until something decodes them.
//!
//! The crate is `no_std` but does require `alloc`.
//!
//! ## Features
//!
//! * `std` enables integration with `std` - mainly `std::error::Error`.
//!
//! ## MSRV
//!
//! 1.63

#![cfg_attr(docsrs, feature(doc_cfg))]
#![no_std]
#![deny(unused_must_use)]
#![deny(missing_docs)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

mod error;
mod params;

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use bitcoin::address::NetworkValidation;
pub use error::{AccessError, Error};
pub use params::{ParamEntry, Params};

/// A parsed `bitcoin:` URI.
///
/// Owned, with no lifetime parameter: every value a [`Uri`] hands back is
/// either copied out of the source string or borrowed from `self`. Query
/// parameters stay percent-encoded and in their original key case in
/// [`Uri::params`] until a typed accessor ([`Uri::label`], [`Uri::amount`],
/// ...) or [`Uri::get`]/[`Uri::get_all`] decodes the ones it needs.
///
/// ## Parsing
///
/// `Uri` implements `FromStr` and `TryFrom<&str>` (equivalent to one
/// another - there is no borrowed/zero-copy parse path in this owned
/// design), producing a [`bitcoin::address::NetworkUnchecked`] `Uri`. Call
/// [`Uri::assume_checked`] to get a `Uri<NetworkChecked>`.
#[derive(Debug, Clone)]
pub struct Uri<V = bitcoin::address::NetworkChecked>
where
    V: NetworkValidation,
{
    body: bitcoin::Address<V>,
    params: Params,
}

/// Two `Uri`s are equal when their body and parameters are.
impl<V: NetworkValidation> PartialEq for Uri<V>
where
    bitcoin::Address<V>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.body == other.body && self.params == other.params
    }
}

impl<V: NetworkValidation> Eq for Uri<V> where bitcoin::Address<V>: Eq {}

impl Uri<bitcoin::address::NetworkUnchecked> {
    /// Parses a `bitcoin:` URI per BIP 321.
    ///
    /// The body address (before the `?`) is required. A query parameter may
    /// be value-less (`?foo` rather than `?foo=`). `label`, `message`,
    /// `pop`, and `amount` may not repeat.
    ///
    /// This does not check the body address against a *specific* network.
    /// Call [`Uri::assume_checked`] to get a checked `Uri`.
    pub fn parse(s: &str) -> Result<Self, Error> {
        const SCHEME: &str = "bitcoin:";

        if !s.get(..SCHEME.len()).map_or(false, |prefix| prefix.eq_ignore_ascii_case(SCHEME)) {
            return Err(Error::InvalidScheme);
        }
        let rest = &s[SCHEME.len()..];

        let (body_str, query) = match rest.find('?') {
            Some(pos) => (&rest[..pos], Some(&rest[(pos + 1)..])),
            None => (rest, None),
        };

        let body = body_str.parse::<bitcoin::Address<_>>().map_err(Error::InvalidBodyAddress)?;

        let mut params = Params::new();
        if let Some(query) = query {
            // RFC 3986 §3.4: the query ends at the first '#', if any.
            let query = match query.find('#') {
                Some(pos) => &query[..pos],
                None => query,
            };

            for segment in query.split('&') {
                // `bitcoinparam = [ ... ]`: an empty segment (e.g. a stray
                // "&&") is a legal, empty parameter that contributes
                // nothing.
                if segment.is_empty() {
                    continue;
                }

                let (key, raw_value) = match segment.find('=') {
                    // `otherparam = qchar *qchar [ "=" *qchar ]`: the value
                    // is optional, so a value-less param is legal.
                    Some(pos) => (&segment[..pos], Some(segment[(pos + 1)..].to_owned())),
                    None => (segment, None),
                };

                if let Some(raw) = raw_value.as_deref() {
                    validate_percent_encoding(key, raw)?;
                }

                let base_key = params::base_key(key);
                if params::NON_REPEATABLE_KEYS
                    .iter()
                    .any(|&non_repeatable| base_key.eq_ignore_ascii_case(non_repeatable))
                    && params.get(base_key).is_some()
                {
                    return Err(Error::DuplicateParameter(key.to_owned()));
                }

                if base_key.eq_ignore_ascii_case("amount") {
                    let raw = raw_value.as_deref().unwrap_or("");
                    bitcoin::Amount::from_str_in(raw, bitcoin::amount::Denomination::Bitcoin).map_err(Error::InvalidAmount)?;
                }

                params.push(ParamEntry {
                    key: key.to_owned(),
                    raw_value,
                });
            }
        }

        Ok(Uri { body, params })
    }

    /// Marks this `Uri` validated without checking its network.
    pub fn assume_checked(self) -> Uri<bitcoin::address::NetworkChecked> {
        Uri {
            body: self.body.assume_checked(),
            params: self.params,
        }
    }
}

impl core::str::FromStr for Uri<bitcoin::address::NetworkUnchecked> {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl<'a> core::convert::TryFrom<&'a str> for Uri<bitcoin::address::NetworkUnchecked> {
    type Error = Error;

    fn try_from(s: &'a str) -> Result<Self, Self::Error> {
        Self::parse(s)
    }
}

impl<V: NetworkValidation> Uri<V> {
    /// The body address (before the `?`).
    pub fn body(&self) -> &bitcoin::Address<V> {
        &self.body
    }

    /// The raw, ordered multimap of every query parameter.
    pub fn params(&self) -> &Params {
        &self.params
    }

    /// The first parameter whose base key matches `key` case-insensitively.
    ///
    /// `req-foo` has base key `foo`, so `get("foo")` finds both `foo` and
    /// `req-foo`; source order decides which entry is first.
    pub fn get(&self, key: &str) -> Option<&ParamEntry> {
        self.params.get(key)
    }

    /// Every parameter whose base key matches `key` case-insensitively, in
    /// source order.
    pub fn get_all<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a ParamEntry> + 'a {
        self.params.get_all(key)
    }

    /// The parsed `amount` parameter, if present.
    pub fn amount(&self) -> Option<bitcoin::Amount> {
        self.params.get("amount").and_then(|entry| {
            let raw = entry.raw_value.as_deref().unwrap_or("");
            bitcoin::Amount::from_str_in(raw, bitcoin::amount::Denomination::Bitcoin).ok()
        })
    }

    /// The decoded `label` parameter, if present.
    pub fn label(&self) -> Option<Result<String, AccessError>> {
        self.decoded_string_param("label")
    }

    /// The decoded `message` parameter, if present.
    pub fn message(&self) -> Option<Result<String, AccessError>> {
        self.decoded_string_param("message")
    }

    /// The decoded `pop` parameter, if present.
    pub fn pop(&self) -> Option<Result<String, AccessError>> {
        self.decoded_string_param("pop")
    }

    fn decoded_string_param(&self, key: &str) -> Option<Result<String, AccessError>> {
        self.params.get(key).and_then(ParamEntry::decoded_value)
    }
}

/// Formats a `Uri<NetworkChecked>` as `bitcoin:<body>?<params>`, with
/// parameters in their original order and key case.
impl fmt::Display for Uri<bitcoin::address::NetworkChecked> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bitcoin:{}", self.body)?;
        if !self.params.is_empty() {
            write!(f, "?{}", self.params)?;
        }
        Ok(())
    }
}

/// Percent-decodes `s` and validates the result as UTF-8.
///
/// This is the strict decode: a byte sequence that isn't valid UTF-8 is
/// itself the error, for an accessor like [`Uri::label`] where that
/// distinction is the whole point.
pub(crate) fn decode_to_string(s: &str) -> Result<String, AccessError> {
    let bytes: Vec<u8> = percent_encoding_rfc3986::percent_decode_str(s)
        .map_err(AccessError::PercentDecode)?
        .collect();
    String::from_utf8(bytes).map_err(|_| AccessError::NotUtf8)
}

/// Validates percent escapes without keeping the decoded value.
fn validate_percent_encoding(key: &str, s: &str) -> Result<(), Error> {
    let _: Vec<u8> = percent_encoding_rfc3986::percent_decode_str(s)
        .map_err(Error::percent_decode(key))?
        .collect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    const LEGACY_MAINNET: &str = "1andreas3batLhQa2FawWjeyjCqyBzypd";
    const BECH32_MAINNET: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";

    #[test]
    fn just_address_parses_with_a_body() {
        let input = alloc::format!("bitcoin:{}", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap().assume_checked();
        assert_eq!(uri.body().to_string(), LEGACY_MAINNET);
        assert!(uri.params().is_empty());
    }

    #[test]
    fn a_body_is_required() {
        // Until the body is made optional, a body-less URI does not parse.
        assert!(Uri::parse("bitcoin:?label=foo").is_err());
        assert!(Uri::parse("bitcoin:").is_err());
    }

    #[test]
    fn a_value_less_param_parses() {
        let input = alloc::format!("bitcoin:{}?foo", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.get("foo").unwrap().raw_value, None);
    }

    #[test]
    fn duplicate_label_message_amount_pop_are_rejected() {
        for key in ["label", "message", "amount", "pop"] {
            let input = alloc::format!("bitcoin:{}?{key}=a&{key}=b", LEGACY_MAINNET, key = key);
            assert!(Uri::parse(&input).is_err(), "expected '{}' to be rejected when duplicated", key);
        }
    }

    #[test]
    fn duplicate_required_non_repeatable_params_are_rejected_by_base_key() {
        for key in ["label", "message", "amount", "pop"] {
            let input = alloc::format!("bitcoin:{}?{key}=a&req-{key}=b", LEGACY_MAINNET, key = key);
            assert!(Uri::parse(&input).is_err(), "expected '{}' to be rejected when duplicated", key);
        }
    }

    #[test]
    fn duplicate_non_special_keys_are_allowed() {
        let input = alloc::format!("bitcoin:{}?bc={}&bc={}", LEGACY_MAINNET, BECH32_MAINNET, BECH32_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.get_all("bc").count(), 2);
    }

    #[test]
    fn amount_label_message_decode() {
        let input = alloc::format!(
            "bitcoin:{}?amount=20.3&label=Luke-Jr&message=Donation%20for%20project%20xyz",
            LEGACY_MAINNET
        );
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.amount().unwrap(), bitcoin::Amount::from_sat(2_030_000_000));
        assert_eq!(uri.label().unwrap().unwrap(), "Luke-Jr");
        assert_eq!(uri.message().unwrap().unwrap(), "Donation for project xyz");
    }

    #[test]
    fn invalid_amount_is_a_parse_error() {
        let input = alloc::format!("bitcoin:{}?amount=not-an-amount", LEGACY_MAINNET);
        let err = Uri::parse(&input).unwrap_err();
        assert!(matches!(err, Error::InvalidAmount(_)));
    }

    #[test]
    fn invalid_percent_encoding_is_a_parse_error() {
        let input = alloc::format!("bitcoin:{}?label=%zz", LEGACY_MAINNET);
        let err = Uri::parse(&input).unwrap_err();
        assert!(matches!(err, Error::InvalidPercentEncoding { ref key, .. } if key == "label"));
    }

    #[test]
    fn a_literal_plus_is_never_treated_as_a_space() {
        let input = alloc::format!("bitcoin:{}?label=a+b", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.label().unwrap().unwrap(), "a+b");
    }

    #[test]
    fn an_unknown_param_parses_with_no_error() {
        let input = alloc::format!("bitcoin:{}?unknown=x", LEGACY_MAINNET);
        assert!(Uri::parse(&input).is_ok());
    }

    #[test]
    fn get_and_get_all_match_required_params_by_base_key() {
        let input = alloc::format!("bitcoin:{}?req-pj=a&pj=b&REQ-SP=c", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.get("pj").unwrap().key, "req-pj");
        let pj_keys: Vec<&str> = uri.get_all("pj").map(|entry| entry.key.as_str()).collect();
        assert_eq!(pj_keys, ["req-pj", "pj"]);
        assert_eq!(uri.get("sp").unwrap().key, "REQ-SP");
    }

    #[test]
    fn display_re_emits_body_and_params() {
        let input = alloc::format!("bitcoin:{}?amount=20.3&label=Luke-Jr", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap().assume_checked();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn a_string_shorter_than_the_scheme_has_an_invalid_scheme() {
        assert!(matches!(Uri::parse("bitcoin"), Err(Error::InvalidScheme)));
    }

    #[test]
    fn empty_segments_between_ampersands_are_ignored() {
        let input = alloc::format!("bitcoin:{}?label=foo&&message=bar", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.params().len(), 2);
    }

    #[test]
    fn not_utf8_value_is_an_access_error() {
        let input = alloc::format!("bitcoin:{}?label=%ff%fe", LEGACY_MAINNET);
        match Uri::parse(&input).unwrap().label() {
            Some(Err(AccessError::NotUtf8)) => {},
            other => panic!("expected NotUtf8, got {:?}", other),
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn error_source_is_present_only_for_wrapping_variants() {
        use std::error::Error as _;
        assert!(Error::InvalidScheme.source().is_none());
        let input = alloc::format!("bitcoin:{}?amount=nope", LEGACY_MAINNET);
        assert!(Uri::parse(&input).unwrap_err().source().is_some());
    }
}
