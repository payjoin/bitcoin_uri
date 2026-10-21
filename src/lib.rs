//! Rust-idiomatic, compliant, flexible and performant bitcoin URI crate.
//!
//! * Rust-idiomatic: owned types, no lifetime parameter.
//! * Compliant: parses the full BIP 321 shape, including a body-less URI.
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

mod builder;
mod error;
mod params;

use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use bitcoin::address::NetworkValidation;
pub use builder::UriBuilder;
pub use error::{AccessError, BuildError, Error, NetworkError};
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
/// [`Uri::require_network`] or [`Uri::assume_checked`] to get a
/// `Uri<NetworkChecked>`.
///
/// ## Displaying
///
/// `Display` with `{}` writes a parsed `Uri<NetworkChecked>` back as exactly
/// the string it was parsed from, which [`Uri::source_str`] also returns.
#[derive(Debug, Clone)]
pub struct Uri<V = bitcoin::address::NetworkChecked>
where
    V: NetworkValidation,
{
    body: Option<bitcoin::Address<V>>,
    params: Params,
    source: Option<String>,
}

/// Two `Uri`s are equal when their body and parameters are, regardless of
/// where each came from. `source` is excluded deliberately: a `Uri` parsed
/// from a string and one built from the same parts are the same URI, even
/// though only the former carries the original text. Everything `source`
/// affects - how `Display` writes a parsed URI, [`Uri::source_str`] - is observable through
/// the other fields or is explicitly a provenance detail, so leaving it out
/// keeps equality meaning "the same URI" rather than "parsed from the same
/// bytes".
impl<V: NetworkValidation> PartialEq for Uri<V>
where
    bitcoin::Address<V>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.body == other.body && self.params == other.params
    }
}

impl<V: NetworkValidation> Eq for Uri<V> where bitcoin::Address<V>: Eq {}

/// Query parameter keys this crate validates as an address with a
/// network-specific Human Readable Part, per BIP 321's
/// "New payment instructions using bech32 or bech32m encodings SHOULD reuse
/// their address format's Human Readable Part as the parameter key" rule.
///
/// These are the segwit HRPs `bitcoin` knows. A value under one of these keys
/// must be a segwit address whose HRP is the key, so `bc=1...` (base58) and
/// `tb=bcrt1...` are rejected. Adding a key changes which URIs parse, so it is
/// a breaking change. Signet shares `tb` with testnet.
pub(crate) const HRP_ADDRESS_KEYS: [(&str, bitcoin::Network); 3] = [
    ("bc", bitcoin::Network::Bitcoin),
    ("tb", bitcoin::Network::Testnet),
    ("bcrt", bitcoin::Network::Regtest),
];

impl Uri<bitcoin::address::NetworkUnchecked> {
    /// Parses a `bitcoin:` URI per BIP 321.
    ///
    /// The body address (before the `?`) is optional - BIP 321 allows a
    /// body-less URI such as `bitcoin:?sp=...` - and a query parameter may
    /// be value-less (`?foo` rather than `?foo=`). `label`, `message`,
    /// `pop`, and `amount` may not repeat. A `bc=`/`tb=`/`bcrt=` parameter's
    /// value must be a segwit address of the network its key names. BIP 321's
    /// invalid example `bitcoin:?bc=tb1...` fails this check.
    ///
    /// This does not check the body address, or any HRP-keyed parameter,
    /// against a *specific* network - that is [`Uri::require_network`]'s
    /// job. A `tb1...` body address and a `bc=`-keyed mainnet address can
    /// both parse out of the same URI; whether that combination is usable
    /// is for the caller to decide once it knows which network it wants.
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

        // BIP 321: the body is optional (`[ bitcoinaddress ]`).
        let body = if body_str.is_empty() {
            None
        } else {
            Some(body_str.parse::<bitcoin::Address<_>>().map_err(Error::InvalidBodyAddress)?)
        };

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

                // Match the base key so a `req-` prefix cannot bypass the
                // network check: `req-bc=tb1...` must still be rejected.
                if let Some(&(_, network)) = HRP_ADDRESS_KEYS.iter().find(|(hrp, _)| base_key.eq_ignore_ascii_case(hrp)) {
                    validate_hrp_param(key, raw_value.as_deref().unwrap_or(""), network)?;
                }

                params.push(ParamEntry {
                    key: key.to_owned(),
                    raw_value,
                });
            }
        }

        Ok(Uri {
            body,
            params,
            source: Some(s.to_owned()),
        })
    }

    /// `true` if the body address, and every `bc=`/`tb=`/`bcrt=` parameter, belongs
    /// to `network`. A URI with no body and no HRP-keyed address parameters
    /// is valid for every network.
    pub fn is_valid_for_network(&self, network: bitcoin::Network) -> bool {
        if let Some(body) = &self.body {
            if !body.is_valid_for_network(network) {
                return false;
            }
        }

        HRP_ADDRESS_KEYS.iter().all(|(hrp, _)| {
            self.params.get_all(hrp).all(|entry| {
                let raw = entry.raw_value.as_deref().unwrap_or("");
                match raw.parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>().ok() {
                    Some(address) => address.is_valid_for_network(network),
                    None => false,
                }
            })
        })
    }

    /// Checks whether this `Uri` - its body address and every
    /// `bc=`/`tb=`/`bcrt=` parameter - is valid for `network`, and if so,
    /// marks it checked.
    ///
    /// For details about this mechanism, see section [*parsing
    /// addresses*](bitcoin::Address#parsing-addresses) on
    /// [`bitcoin::Address`].
    pub fn require_network(self, network: bitcoin::Network) -> Result<Uri<bitcoin::address::NetworkChecked>, NetworkError> {
        if !self.is_valid_for_network(network) {
            return Err(NetworkError::NetworkMismatch);
        }
        Ok(self.assume_checked())
    }

    /// Marks this `Uri` validated without checking its network.
    pub fn assume_checked(self) -> Uri<bitcoin::address::NetworkChecked> {
        Uri {
            body: self.body.map(bitcoin::Address::assume_checked),
            params: self.params,
            source: self.source,
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
    /// The body address (before the `?`), if the URI had one. BIP 321 makes
    /// this optional, unlike BIP 21.
    pub fn body(&self) -> Option<&bitcoin::Address<V>> {
        self.body.as_ref()
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

    /// The `req-` parameters this URI requires but the caller does not
    /// understand, in source order.
    ///
    /// BIP 321 says a client MUST reject a URI carrying a `req-` parameter it
    /// cannot handle. `understood` is the set of keys the caller does handle,
    /// given without the `req-` prefix (`["pj", "sp"]`, not `["req-pj"]`) and
    /// matched case-insensitively; every `req-` parameter whose stripped key
    /// is absent from it is returned. An empty iterator means the URI is safe
    /// to act on.
    ///
    /// This crate never enforces the rule itself - it only reports it - so a
    /// `Uri` always parses regardless of its `req-` parameters, and the
    /// caller decides what to do with any this returns.
    #[must_use = "callers must reject a URI when this iterator is non-empty"]
    pub fn unsatisfied_requirements<'a>(&'a self, understood: &'a [&str]) -> impl Iterator<Item = &'a ParamEntry> + 'a {
        self.params.iter().filter(move |entry| match entry.requirement() {
            Some(requirement) => !understood.iter().any(|key| key.eq_ignore_ascii_case(requirement)),
            None => false,
        })
    }

    /// The exact string this `Uri` was parsed from, when it was parsed
    /// rather than built. `None` for a `Uri` assembled by hand.
    pub fn source_str(&self) -> Option<&str> {
        self.source.as_deref()
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

impl Uri<bitcoin::address::NetworkChecked> {
    /// Assembles a checked `Uri` from parts, with no source string.
    ///
    /// The only constructor of a built (as opposed to parsed) `Uri`; used by
    /// [`UriBuilder::build`]. `source` is `None`, so
    /// [`source_str`](Self::source_str) returns `None` and callers fall back
    /// to [`Display`](fmt::Display) for the string form.
    pub(crate) fn from_checked_parts(body: Option<bitcoin::Address>, params: Params) -> Self {
        Uri { body, params, source: None }
    }
}

/// Formats a `Uri<NetworkChecked>`.
///
/// A parsed `Uri` is written back byte for byte: scheme case, an uppercase
/// bech32 body, empty parameters, a trailing `&` or `?` and any `#fragment`
/// all survive. A `Uri` cannot be changed after parsing, so its source is
/// always the URI it describes.
///
/// A `Uri` with no source (one from [`UriBuilder::build`]) is written as
/// `bitcoin:<body>?<params>`, with its parameters in order. The body goes
/// through the [`bitcoin::Address`] `Display`, which is lowercase for `{}`
/// and uppercase for the alternate `{:#}` (QR alphanumeric mode). `{:#}`
/// always formats this way, even for a parsed `Uri`.
impl fmt::Display for Uri<bitcoin::address::NetworkChecked> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(source) = self.source.as_deref().filter(|_| !f.alternate()) {
            return f.write_str(source);
        }
        f.write_str("bitcoin:")?;
        match &self.body {
            Some(body) if f.alternate() => write!(f, "{:#}", body)?,
            Some(body) => write!(f, "{}", body)?,
            None => {},
        }
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

/// Validates an HRP-keyed address parameter: its value must be a segwit
/// address of `network`, the network its key names.
///
/// `require_network` alone is not enough, since a base58 address only carries
/// mainnet or "test", so `1...` would pass for `bc` and `m...` for `tb` and
/// `bcrt`. Requiring a witness program rules out base58, and for a segwit
/// address `require_network` checks the HRP: `tb1...` is not valid for
/// regtest, nor `bcrt1...` for testnet.
///
/// Used both at parse time and by [`UriBuilder::build`], so a built URI is
/// held to the same rule a parsed one is and round-trips.
pub(crate) fn validate_hrp_param(key: &str, raw_value: &str, network: bitcoin::Network) -> Result<(), Error> {
    let is_segwit_on_network = raw_value
        .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
        .ok()
        .and_then(|address| address.require_network(network).ok())
        .map_or(false, |address| address.witness_program().is_some());
    if is_segwit_on_network {
        Ok(())
    } else {
        Err(Error::InvalidHrpValue { key: key.to_owned() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    const LEGACY_MAINNET: &str = "1andreas3batLhQa2FawWjeyjCqyBzypd";
    const BECH32_MAINNET: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
    const BECH32_TESTNET: &str = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx";

    #[test]
    fn just_address_parses_with_a_body() {
        let input = alloc::format!("bitcoin:{}", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap().assume_checked();
        assert_eq!(uri.body().unwrap().to_string(), LEGACY_MAINNET);
        assert!(uri.params().is_empty());
    }

    #[test]
    fn a_body_less_uri_parses() {
        // BIP 321's own example: a silent-payment-only URI has no body.
        let uri = Uri::parse("bitcoin:?sp=sp1qsilentpaymentaddress").unwrap().assume_checked();
        assert!(uri.body().is_none());
        assert_eq!(uri.get("sp").unwrap().raw_value.as_deref(), Some("sp1qsilentpaymentaddress"));
    }

    #[test]
    fn the_bare_scheme_parses_with_no_body() {
        let uri = Uri::parse("bitcoin:").unwrap();
        assert!(uri.body().is_none());
        assert!(uri.params().is_empty());
    }

    #[test]
    fn display_of_a_body_less_uri_has_no_address_before_the_question_mark() {
        let input = "bitcoin:?sp=sp1qsilentpaymentaddress";
        let uri = Uri::parse(input).unwrap().assume_checked();
        assert_eq!(uri.to_string(), input);
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

    #[test]
    fn a_mainnet_address_under_the_bc_key_parses() {
        let input = alloc::format!("bitcoin:?bc={}", BECH32_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.get("bc").unwrap().raw_value.as_deref(), Some(BECH32_MAINNET));
    }

    #[test]
    fn a_testnet_address_under_the_bc_key_is_rejected() {
        // BIP 321's own invalid example: `bitcoin:?bc=tb1...`.
        let input = alloc::format!("bitcoin:?bc={}", BECH32_TESTNET);
        assert!(Uri::parse(&input).is_err());
    }

    #[test]
    fn a_testnet_address_under_the_tb_key_parses() {
        let input = alloc::format!("bitcoin:?tb={}", BECH32_TESTNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.get("tb").unwrap().raw_value.as_deref(), Some(BECH32_TESTNET));
    }

    #[test]
    fn a_req_prefixed_hrp_param_is_still_network_checked() {
        // `req-bc` has base key `bc` (mainnet); a testnet value must not slip
        // through because of the `req-` prefix.
        let input = alloc::format!("bitcoin:?req-bc={}", BECH32_TESTNET);
        assert!(Uri::parse(&input).is_err());
    }

    fn address_on(address: &str, network: bitcoin::Network) -> String {
        let script = address.parse::<bitcoin::Address<_>>().unwrap().assume_checked().script_pubkey();
        bitcoin::Address::from_script(&script, network).unwrap().to_string()
    }

    #[test]
    fn a_regtest_address_under_the_bcrt_key_parses() {
        let regtest = address_on(BECH32_MAINNET, bitcoin::Network::Regtest);
        assert!(regtest.starts_with("bcrt1"));
        let uri = Uri::parse(&alloc::format!("bitcoin:?bcrt={}", regtest)).unwrap();
        assert!(uri.is_valid_for_network(bitcoin::Network::Regtest));
        assert!(uri.clone().require_network(bitcoin::Network::Bitcoin).is_err());
        assert!(uri.require_network(bitcoin::Network::Testnet).is_err());
    }

    #[test]
    fn an_hrp_key_takes_only_a_segwit_address_whose_hrp_is_the_key() {
        let regtest = address_on(BECH32_MAINNET, bitcoin::Network::Regtest);
        let testnet_base58 = address_on(LEGACY_MAINNET, bitcoin::Network::Testnet);
        for (key, value) in [
            ("bc", LEGACY_MAINNET),
            ("bc", BECH32_TESTNET),
            ("bc", regtest.as_str()),
            ("tb", testnet_base58.as_str()),
            ("tb", regtest.as_str()),
            ("tb", BECH32_MAINNET),
            ("bcrt", BECH32_TESTNET),
            ("bcrt", testnet_base58.as_str()),
            ("bcrt", "garbage"),
            ("REQ-BCRT", BECH32_TESTNET),
        ] {
            let input = alloc::format!("bitcoin:?{}={}", key, value);
            match Uri::parse(&input) {
                Err(Error::InvalidHrpValue { key: ref reported }) => assert_eq!(reported, key),
                other => panic!("{} should fail with InvalidHrpValue, got {:?}", input, other),
            }
        }
    }

    #[test]
    fn hrp_keys_and_values_are_case_insensitive() {
        // BIP 321's all-uppercase QR example.
        let uri = Uri::parse("BITCOIN:?BC=BC1QUFGY354J3KMVUCH987XE4S40836X3H0LG8F5N2").unwrap();
        assert!(uri.is_valid_for_network(bitcoin::Network::Bitcoin));
    }

    #[test]
    fn require_network_rejects_a_body_on_the_wrong_network() {
        let input = alloc::format!("bitcoin:{}", BECH32_TESTNET);
        let uri = Uri::parse(&input).unwrap();
        assert!(uri.require_network(bitcoin::Network::Bitcoin).is_err());
    }

    #[test]
    fn require_network_accepts_a_matching_body_and_hrp_param() {
        let input = alloc::format!("bitcoin:{}?bc={}", BECH32_MAINNET, BECH32_MAINNET);
        let uri = Uri::parse(&input).unwrap().require_network(bitcoin::Network::Bitcoin).unwrap();
        assert!(uri.body().is_some());
    }

    #[test]
    fn require_network_rejects_a_mismatched_hrp_param_even_with_a_matching_body() {
        let input = alloc::format!("bitcoin:{}?tb={}", BECH32_MAINNET, BECH32_TESTNET);
        let uri = Uri::parse(&input).unwrap();
        assert!(uri.require_network(bitcoin::Network::Bitcoin).is_err());
    }

    #[test]
    fn network_error_display_message_is_specific() {
        assert_eq!(
            NetworkError::NetworkMismatch.to_string(),
            "the URI is not valid for the requested network"
        );
    }

    #[test]
    fn an_unclaimed_req_param_does_not_fail_parsing() {
        // Unlike 0.1, the parser never enforces `req-` itself; it is
        // reported by `unsatisfied_requirements` and left to the caller.
        let input = alloc::format!("bitcoin:{}?req-somethingyoudontunderstand=50", LEGACY_MAINNET);
        assert!(Uri::parse(&input).is_ok());
    }

    #[test]
    fn an_unknown_req_param_is_unsatisfied() {
        let input = alloc::format!("bitcoin:{}?req-foo=bar", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        let unsatisfied: Vec<&str> = uri.unsatisfied_requirements(&[]).map(|e| e.key.as_str()).collect();
        assert_eq!(unsatisfied, ["req-foo"]);
    }

    #[test]
    fn a_req_param_the_caller_understands_is_satisfied() {
        let input = alloc::format!("bitcoin:{}?req-pj=https://example.com", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.unsatisfied_requirements(&["pj"]).count(), 0);
    }

    #[test]
    fn understood_keys_are_matched_case_insensitively_and_without_the_prefix() {
        let input = alloc::format!("bitcoin:{}?REQ-PJ=x", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        // Given without the `req-` prefix, matched ignoring case.
        assert_eq!(uri.unsatisfied_requirements(&["pj"]).count(), 0);
        assert_eq!(uri.unsatisfied_requirements(&[]).count(), 1);
    }

    #[test]
    fn plain_params_are_never_unsatisfied_requirements() {
        let input = alloc::format!("bitcoin:{}?label=foo&pj=x&amount=1", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        // None of these carry a `req-` prefix, so none are requirements.
        assert_eq!(uri.unsatisfied_requirements(&[]).count(), 0);
    }

    #[test]
    fn only_the_unsatisfied_requirements_surface_in_source_order() {
        let input = alloc::format!("bitcoin:{}?req-pj=a&req-foo=b&sp=c&req-bar=d", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        let unsatisfied: Vec<&str> = uri.unsatisfied_requirements(&["pj"]).map(|e| e.key.as_str()).collect();
        assert_eq!(unsatisfied, ["req-foo", "req-bar"]);
    }

    #[test]
    fn a_required_pop_is_reported_like_any_other_requirement() {
        let input = alloc::format!("bitcoin:{}?req-pop=https://example.com/pop", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.unsatisfied_requirements(&[]).count(), 1);
        assert_eq!(uri.unsatisfied_requirements(&["pop"]).count(), 0);
    }

    #[test]
    fn get_finds_a_req_prefixed_key_by_base_key() {
        // `req-` is only a prefix, so a consumer that understands `sp` finds it
        // whether or not it is required.
        let uri = Uri::parse("bitcoin:?req-sp=sp1silent").unwrap();
        assert_eq!(uri.get("sp").expect("get should match req-sp").key, "req-sp");
    }

    #[test]
    fn display_of_a_parsed_uri_is_its_input() {
        let bodies = ["", LEGACY_MAINNET, BECH32_MAINNET, "BC1QW508D6QEJXTDG4Y5R3ZARVARY0C5XW7KV8F3T4"];
        let queries = [
            None,
            Some(""),
            Some("&"),
            Some("a"),
            Some("a="),
            Some("a=b"),
            Some("a&&b"),
            Some("label=x&"),
            Some("LABEL=Luke-Jr&Amount=1.5&req-pj=https://e.com/?x=a+b%23c"),
            Some("bc=bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4&bc=bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"),
        ];
        for scheme in ["bitcoin:", "BITCOIN:", "BitCoin:"] {
            for body in bodies {
                for query in queries {
                    for fragment in [None, Some(""), Some("frag")] {
                        let mut input = alloc::format!("{}{}", scheme, body);
                        if let Some(query) = query {
                            input = alloc::format!("{}?{}", input, query);
                            // Without a query, a `#` would end up in the body.
                            if let Some(fragment) = fragment {
                                input = alloc::format!("{}#{}", input, fragment);
                            }
                        }
                        let uri = Uri::parse(&input).unwrap_or_else(|e| panic!("{} should parse: {:?}", input, e));
                        assert_eq!(uri.assume_checked().to_string(), input);
                    }
                }
            }
        }
    }

    #[test]
    fn alternate_display_of_a_parsed_uri_uppercases_the_body() {
        let input = alloc::format!("BITCOIN:{}?label=Luke-Jr", BECH32_MAINNET);
        let uri = Uri::parse(&input).unwrap().assume_checked();
        let expected = alloc::format!("bitcoin:{}?label=Luke-Jr", BECH32_MAINNET.to_uppercase());
        assert_eq!(alloc::format!("{:#}", uri), expected);
    }

    #[test]
    fn source_str_returns_the_exact_input() {
        let input = alloc::format!("bitcoin:{}?label=Luke-Jr", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        assert_eq!(uri.source_str(), Some(input.as_str()));
    }

    #[test]
    fn uppercase_bech32_body_survives_the_round_trip() {
        // QR alphanumeric mode carries a bech32 address in uppercase. The
        // `Address` Display would lowercase it, so the body must come back as
        // written.
        let input = alloc::format!("BITCOIN:{}", BECH32_MAINNET.to_uppercase());
        let uri = Uri::parse(&input).unwrap().assume_checked();
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn a_literal_plus_inside_pj_survives_parse_and_re_emission() {
        let input = "bitcoin:?pj=https://example.com/?x=a+b";
        let uri = Uri::parse(input).unwrap().assume_checked();
        assert_eq!(uri.get("pj").unwrap().raw_value.as_deref(), Some("https://example.com/?x=a+b"));
        assert_eq!(uri.to_string(), input);
    }

    #[test]
    fn issue_10_round_trip_regression_passes() {
        let valid_uri = "bitcoin:12c6DSiU4Rq3P4ZxziKxzrL5LmMBrzjrJX?amount=0.01&pjos=0&pj=HTTPS://EXAMPLE.COM/%23OH1QYPM5JXYNS754Y4R45QWE336QFX6ZR8DQGVQCULVZTV20TFVEYDMFQC";
        let round_trip = valid_uri.parse::<Uri<_>>().unwrap().assume_checked().to_string();
        assert_eq!(valid_uri, round_trip);

        let uri = "Bitcoin:12c6DSiU4Rq3P4ZxziKxzrL5LmMBrzjrJX?6=q3Ҁf&pj=\u{3}\0\0\0\0\0\0http\r\r\r\r\r\r\r\r\r:\r\r\\.onion\\\\\\\\\\q3";
        let round_trip = uri.parse::<Uri<_>>().unwrap().assume_checked().to_string();
        assert_eq!(uri, round_trip);
    }

    #[test]
    fn equality_is_by_body_and_params_not_provenance() {
        let input = alloc::format!("bitcoin:{}", BECH32_MAINNET);
        let parsed = Uri::parse(&input).unwrap().require_network(bitcoin::Network::Bitcoin).unwrap();
        let address = BECH32_MAINNET
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
            .unwrap()
            .assume_checked();
        let built = UriBuilder::new().address(address).build().unwrap();
        // Different provenance (parsed carries a source, built does not) but
        // the same body and params: equal.
        assert!(parsed.source_str().is_some());
        assert!(built.source_str().is_none());
        assert_eq!(parsed, built);

        // Same params, different body: not equal.
        let other_body = Uri::parse(&alloc::format!("bitcoin:{}", LEGACY_MAINNET))
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap();
        assert_ne!(parsed, other_body);

        // Same body, different params: not equal.
        let with_param = Uri::parse(&alloc::format!("bitcoin:{}?label=x", BECH32_MAINNET))
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap();
        assert_ne!(parsed, with_param);
    }

    #[test]
    #[cfg(feature = "std")]
    fn build_error_source_is_present_only_for_the_network_variant() {
        use std::error::Error as _;
        assert!(BuildError::Network(NetworkError::NetworkMismatch).source().is_some());
        assert!(BuildError::InvalidKey("x".to_string()).source().is_none());
    }

    #[test]
    fn alternate_display_of_a_built_uri_uppercases_the_body() {
        // A built URI has no source, so the body goes through the `Address`
        // Display: `{}` is lowercase, `{:#}` is uppercase (QR alphanumeric
        // mode). The alternate flag must pick between them.
        let address = BECH32_MAINNET
            .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
            .unwrap()
            .assume_checked();
        let uri = UriBuilder::new().address(address).build().unwrap();
        assert_eq!(alloc::format!("{}", uri), alloc::format!("bitcoin:{}", BECH32_MAINNET));
        assert_eq!(alloc::format!("{:#}", uri), alloc::format!("bitcoin:{}", BECH32_MAINNET.to_uppercase()));
    }

    #[test]
    fn error_display_messages_are_distinct_and_mention_the_key() {
        let bad_scheme = Error::InvalidScheme.to_string();
        let dup = Error::DuplicateParameter("label".to_owned()).to_string();
        let amount = Uri::parse("bitcoin:?amount=nope").unwrap_err().to_string();
        assert_ne!(bad_scheme, amount);
        assert!(dup.contains("label"));
    }

    #[test]
    fn access_error_display_messages_are_specific() {
        assert_eq!(AccessError::NotUtf8.to_string(), "value is not valid UTF-8");
        let decode_err = percent_encoding_rfc3986::percent_decode_str("%zz").unwrap_err();
        assert_eq!(AccessError::PercentDecode(decode_err).to_string(), "invalid percent-encoding");
    }

    #[test]
    #[cfg(feature = "std")]
    fn access_error_source_matches_its_variant() {
        use std::error::Error as _;
        assert!(AccessError::NotUtf8.source().is_none());
        let decode_error = percent_encoding_rfc3986::percent_decode_str("%zz").unwrap_err();
        assert!(AccessError::PercentDecode(decode_error).source().is_some());
    }

    #[test]
    fn params_iter_and_into_iter_agree_and_see_every_entry() {
        let input = alloc::format!("bitcoin:{}?label=foo&message=bar", LEGACY_MAINNET);
        let uri = Uri::parse(&input).unwrap();
        let via_iter: Vec<&str> = uri.params().iter().map(|entry| entry.key.as_str()).collect();
        let via_into_iter: Vec<&str> = uri.params().into_iter().map(|entry| entry.key.as_str()).collect();
        assert_eq!(via_iter, ["label", "message"]);
        assert_eq!(via_iter, via_into_iter);
    }

    #[test]
    fn a_param_value_is_tokenized_verbatim_and_decoded_on_demand() {
        // The crate tokenizes: the raw value stays percent-encoded and the
        // consumer decodes with `decoded_value` when it wants the text. `+` is
        // a literal plus, never a space.
        let uri = Uri::parse("bitcoin:?lightning=a+b%20c").unwrap();
        let entry = uri.get("lightning").unwrap();
        assert_eq!(entry.raw_value.as_deref(), Some("a+b%20c"));
        assert_eq!(entry.decoded_value().unwrap().unwrap(), "a+b c");
    }

    #[test]
    fn decoded_value_is_none_for_a_value_less_param() {
        let uri = Uri::parse("bitcoin:?flag").unwrap();
        assert!(uri.get("flag").unwrap().decoded_value().is_none());
    }

    #[test]
    fn pop_decodes_its_percent_encoded_value() {
        let input = "bitcoin:?pop=hello%20world";
        let uri = Uri::parse(input).unwrap();
        assert_eq!(uri.pop().unwrap().unwrap(), "hello world");
    }

    #[test]
    fn pop_is_none_when_absent() {
        let uri = Uri::parse("bitcoin:?label=Luke-Jr").unwrap();
        assert!(uri.pop().is_none());
    }

    #[test]
    fn bad_unicode_scheme_is_rejected() {
        let input = "bitcoinö:1andreas3batLhQa2FawWjeyjCqyBzypd";
        assert!(Uri::parse(input).is_err());
    }

    #[test]
    fn a_bodyless_uri_with_only_unknown_params_parses() {
        let uri = Uri::parse("bitcoin:?unknown=x")
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap();
        assert!(uri.body().is_none());
        assert_eq!(uri.get("unknown").unwrap().raw_value.as_deref(), Some("x"));
    }

    #[test]
    fn mixed_extension_keys_all_survive_parsing() {
        let input = "bitcoin:?lno=lno1bogusoffer&sp=sp1qsilentpaymentaddress";
        let uri = Uri::parse(input).unwrap();
        assert!(uri.body().is_none());
        assert_eq!(uri.get("lno").unwrap().raw_value.as_deref(), Some("lno1bogusoffer"));
        assert_eq!(uri.get("sp").unwrap().raw_value.as_deref(), Some("sp1qsilentpaymentaddress"));
    }
}
