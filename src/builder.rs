//! Assembling a `bitcoin:` URI from typed parts.
//!
//! [`UriBuilder`] is the counterpart to parsing: it takes an address, an
//! amount, labels, and any extension parameters and produces a [`Uri`] whose
//! [`Display`](core::fmt::Display) re-parses to the same parameters.
//!
//! Values handed to the typed setters and to [`UriBuilder::param`] are
//! percent-encoded for you; [`UriBuilder::raw_param`] takes a value that is
//! already encoded and passes it through untouched. Encoding is RFC 3986: a
//! literal `+` becomes `%2B` rather than being left to a reader that might
//! read it as a space.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use percent_encoding_rfc3986::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

use crate::error::{BuildError, NetworkError};
use crate::params::{self, ParamEntry, Params, NON_REPEATABLE_KEYS};
use crate::{Uri, HRP_ADDRESS_KEYS};

/// The bytes a parameter value is percent-encoded against.
///
/// Starts from [`NON_ALPHANUMERIC`] and allows back the `unreserved`
/// characters and the `sub-delims` / `pchar` extras that are safe inside a
/// BIP 321 query value - but keeps `&` and `=` (the parameter separators)
/// encoded, and deliberately keeps `+` encoded as `%2B` so no reader can
/// mistake it for a space.
const VALUE_ASCII_SET: AsciiSet = NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~')
    .remove(b'!')
    .remove(b'$')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'*')
    .remove(b',')
    .remove(b';')
    .remove(b':')
    .remove(b'@')
    .remove(b'/')
    .remove(b'?');

fn encode(value: &str) -> String {
    utf8_percent_encode(value, &VALUE_ASCII_SET).to_string()
}

fn key_round_trips(key: &str) -> bool {
    !key.is_empty() && !key.bytes().any(|byte| matches!(byte, b'&' | b'=' | b'#'))
}

fn raw_value_round_trips(value: &str) -> bool {
    !value.bytes().any(|byte| matches!(byte, b'&' | b'#')) && percent_encoding_rfc3986::percent_decode_str(value).is_ok()
}

fn is_non_repeatable_base_key(key: &str, base_key: &str) -> bool {
    params::base_key(key).eq_ignore_ascii_case(base_key)
}

/// Builds a [`Uri`] from typed parts.
///
/// The setters are chainable and consume `self`. A non-repeatable parameter
/// (`amount`, `label`, `message`) set more than once through its own setter
/// keeps only the last value; a duplicate introduced through [`param`] or
/// [`raw_param`] is caught by [`build`] instead.
///
/// [`param`]: UriBuilder::param
/// [`raw_param`]: UriBuilder::raw_param
/// [`build`]: UriBuilder::build
///
/// ```
/// # use bitcoin_uri::{UriBuilder, Uri};
/// let address = "1andreas3batLhQa2FawWjeyjCqyBzypd".parse::<bitcoin::Address<_>>().unwrap().assume_checked();
/// let uri = UriBuilder::new()
///     .address(address)
///     .amount(bitcoin::Amount::from_sat(100_000))
///     .label("Luke-Jr")
///     .build(bitcoin::Network::Bitcoin)
///     .unwrap();
/// assert!(uri.to_string().starts_with("bitcoin:1andreas3batLhQa2FawWjeyjCqyBzypd?"));
/// ```
#[derive(Debug, Clone, Default)]
#[must_use = "a UriBuilder builds nothing until `.build(bitcoin::Network::Bitcoin)` is called"]
pub struct UriBuilder {
    body: Option<bitcoin::Address>,
    entries: Vec<ParamEntry>,
}

impl UriBuilder {
    /// A builder for a URI with no body and no parameters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the body address (the part before the `?`), replacing any
    /// previously set one.
    pub fn address(mut self, address: bitcoin::Address) -> Self {
        self.body = Some(address);
        self
    }

    /// Sets the `amount` parameter, formatted in BTC, replacing any previous
    /// one.
    pub fn amount(mut self, amount: bitcoin::Amount) -> Self {
        let value = amount.to_string_in(bitcoin::amount::Denomination::Bitcoin);
        self.set_unique("amount", value);
        self
    }

    /// Sets the `label` parameter, percent-encoding the value and replacing
    /// any previous one.
    pub fn label(mut self, label: &str) -> Self {
        self.set_unique("label", encode(label));
        self
    }

    /// Sets the `message` parameter, percent-encoding the value and
    /// replacing any previous one.
    pub fn message(mut self, message: &str) -> Self {
        self.set_unique("message", encode(message));
        self
    }

    /// Appends an extension parameter, percent-encoding the value.
    ///
    /// Use this for any key this crate has no typed setter for - a payment
    /// key such as `bc`, `tb`, `lightning`, or `sp`, or any other extension.
    /// A `bc=`/`tb=` value is validated as an address of that network by
    /// [`build`](Self::build), so the built URI parses back. The value is
    /// treated as decoded text and encoded for you.
    pub fn param(mut self, key: &str, value: &str) -> Self {
        self.push(key, encode(value));
        self
    }

    /// Appends an arbitrary parameter whose value is already percent-encoded.
    ///
    /// The value is written verbatim, so the caller owns its encoding. An
    /// empty string produces a `key=` parameter with an empty value, not a
    /// value-less `key`.
    pub fn raw_param(mut self, key: &str, raw_value: &str) -> Self {
        self.push(key, raw_value.to_string());
        self
    }

    /// Finishes building for `network`, or reports why the parts do not form a
    /// valid URI.
    ///
    /// Fails if a non-repeatable parameter (`amount`/`label`/`message`/`pop`)
    /// was set more than once, or if a key or raw value would not round-trip.
    /// The body and every `bc=`/`tb=` address must belong to `network`, so a
    /// built URI never mixes networks - the same guarantee
    /// [`Uri::require_network`] gives a parsed one.
    pub fn build(self, network: bitcoin::Network) -> Result<Uri, BuildError> {
        for entry in &self.entries {
            if !key_round_trips(&entry.key) {
                return Err(BuildError::InvalidKey(entry.key.clone()));
            }
            if let Some(raw_value) = entry.raw_value.as_deref() {
                if !raw_value_round_trips(raw_value) {
                    return Err(BuildError::InvalidRawValue { key: entry.key.clone() });
                }
            }
        }

        for &key in &NON_REPEATABLE_KEYS {
            if self.entries.iter().filter(|entry| entry.base_key_eq(key)).count() > 1 {
                return Err(BuildError::DuplicateParameter(key.to_string()));
            }
        }

        for entry in &self.entries {
            if is_non_repeatable_base_key(&entry.key, "amount") {
                let raw = entry.raw_value.as_deref().unwrap_or("");
                bitcoin::Amount::from_str_in(raw, bitcoin::amount::Denomination::Bitcoin)
                    .map_err(|_| BuildError::InvalidRawValue { key: entry.key.clone() })?;
            }
        }

        // The body and every HRP-keyed address must belong to `network`, so a
        // checked URI never mixes networks and parses back unchanged.
        if let Some(body) = &self.body {
            if !body.as_unchecked().is_valid_for_network(network) {
                return Err(BuildError::Network(NetworkError::NetworkMismatch));
            }
        }
        for entry in &self.entries {
            if HRP_ADDRESS_KEYS.iter().any(|(hrp, _)| entry.base_key_eq(hrp)) {
                let raw = entry.raw_value.as_deref().unwrap_or("");
                let address = raw
                    .parse::<bitcoin::Address<bitcoin::address::NetworkUnchecked>>()
                    .map_err(|_| BuildError::InvalidHrpValue { key: entry.key.clone() })?;
                if !address.is_valid_for_network(network) {
                    return Err(BuildError::Network(NetworkError::NetworkMismatch));
                }
            }
        }

        let mut params = Params::new();
        for entry in self.entries {
            params.push(entry);
        }
        Ok(Uri::from_checked_parts(self.body, params))
    }

    fn push(&mut self, key: &str, value: String) {
        self.entries.push(ParamEntry {
            key: key.to_string(),
            raw_value: Some(value),
        });
    }

    fn set_unique(&mut self, key: &str, value: String) {
        match self.entries.iter_mut().find(|entry| entry.key_eq(key)) {
            Some(entry) => entry.raw_value = Some(value),
            None => self.push(key, value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AccessError;

    const LEGACY_MAINNET: &str = "1andreas3batLhQa2FawWjeyjCqyBzypd";
    const BECH32_MAINNET: &str = "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4";
    const BECH32_TESTNET: &str = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx";

    fn mainnet_body() -> bitcoin::Address {
        LEGACY_MAINNET.parse::<bitcoin::Address<_>>().unwrap().assume_checked()
    }

    #[test]
    fn an_empty_builder_produces_the_bare_scheme() {
        let uri = UriBuilder::new().build(bitcoin::Network::Bitcoin).unwrap();
        assert_eq!(uri.to_string(), "bitcoin:");
        assert!(uri.body().is_none());
        assert!(uri.params().is_empty());
    }

    #[test]
    fn a_built_uri_has_no_source_string() {
        let uri = UriBuilder::new().address(mainnet_body()).build(bitcoin::Network::Bitcoin).unwrap();
        assert!(uri.source_str().is_none());
    }

    #[test]
    fn the_typed_setters_land_as_their_parameters() {
        let uri = UriBuilder::new()
            .address(mainnet_body())
            .amount(bitcoin::Amount::from_sat(20_300_000))
            .label("Luke-Jr")
            .message("Donation")
            .build(bitcoin::Network::Bitcoin)
            .unwrap();
        assert_eq!(uri.body().unwrap().to_string(), LEGACY_MAINNET);
        assert_eq!(uri.amount().unwrap(), bitcoin::Amount::from_sat(20_300_000));
        assert_eq!(uri.label().unwrap().unwrap(), "Luke-Jr");
        assert_eq!(uri.message().unwrap().unwrap(), "Donation");
    }

    #[test]
    fn a_value_is_percent_encoded_but_raw_value_is_not() {
        let uri = UriBuilder::new()
            .param("x", "a b&c")
            .raw_param("y", "a%20b")
            .build(bitcoin::Network::Bitcoin)
            .unwrap();
        assert_eq!(uri.get("x").unwrap().raw_value.as_deref(), Some("a%20b%26c"));
        assert_eq!(uri.get("y").unwrap().raw_value.as_deref(), Some("a%20b"));
    }

    #[test]
    fn a_literal_plus_is_encoded_as_percent_2b() {
        let uri = UriBuilder::new()
            .param("pj", "https://ex.com/?x=a+b")
            .build(bitcoin::Network::Bitcoin)
            .unwrap();
        let raw = uri.get("pj").unwrap().raw_value.as_deref().unwrap();
        assert!(raw.contains("%2B"), "expected + to encode as %2B, got {}", raw);
        assert!(!raw.contains('+'), "no literal + should remain, got {}", raw);
    }

    #[test]
    fn setting_a_unique_key_twice_keeps_the_last_value() {
        let uri = UriBuilder::new().label("first").label("second").build(bitcoin::Network::Bitcoin).unwrap();
        assert_eq!(uri.get_all("label").count(), 1);
        assert_eq!(uri.label().unwrap().unwrap(), "second");
    }

    #[test]
    fn a_duplicate_non_repeatable_key_via_raw_param_fails_to_build() {
        let result = UriBuilder::new().label("ok").raw_param("label", "again").build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::DuplicateParameter(ref key)) if key == "label"));
    }

    #[test]
    fn a_duplicate_required_non_repeatable_key_fails_to_build() {
        let result = UriBuilder::new()
            .label("ok")
            .raw_param("req-label", "again")
            .build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::DuplicateParameter(ref key)) if key == "label"));
    }

    #[test]
    fn ambiguous_keys_fail_to_build() {
        let result = UriBuilder::new().param("a&b", "x").build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::InvalidKey(ref key)) if key == "a&b"));

        let result = UriBuilder::new().param("a=b", "x").build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::InvalidKey(ref key)) if key == "a=b"));
    }

    #[test]
    fn raw_values_that_would_not_round_trip_fail_to_build() {
        let result = UriBuilder::new().raw_param("x", "a&b").build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::InvalidRawValue { ref key }) if key == "x"));

        let result = UriBuilder::new().raw_param("x", "%zz").build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::InvalidRawValue { ref key }) if key == "x"));
    }

    #[test]
    fn a_manually_added_invalid_amount_fails_to_build() {
        let result = UriBuilder::new().raw_param("amount", "not-an-amount").build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::InvalidRawValue { ref key }) if key == "amount"));
    }

    #[test]
    fn an_hrp_param_lands_under_its_key() {
        let uri = UriBuilder::new().param("bc", BECH32_MAINNET).build(bitcoin::Network::Bitcoin).unwrap();
        assert_eq!(uri.get("bc").unwrap().raw_value.as_deref(), Some(BECH32_MAINNET));
    }

    #[test]
    fn mixed_networks_fail_to_build() {
        // A checked URI never mixes networks: `bc=` (mainnet) and `tb=`
        // (testnet) cannot both belong to one network, so the build fails.
        let result = UriBuilder::new()
            .param("bc", BECH32_MAINNET)
            .param("tb", BECH32_TESTNET)
            .build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::Network(_))));
    }

    #[test]
    fn an_hrp_value_on_the_wrong_network_fails_to_build() {
        // BIP 321's own invalid example: a testnet address under the `bc` key.
        let result = UriBuilder::new().param("bc", BECH32_TESTNET).build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::Network(_))));
    }

    #[test]
    fn the_extension_params_become_their_keys_with_encoded_values() {
        let uri = UriBuilder::new()
            .param("lightning", "ln bc")
            .param("lno", "lno1")
            .param("pay", "pay1")
            .param("sp", "sp1")
            .build(bitcoin::Network::Bitcoin)
            .unwrap();
        assert_eq!(uri.get("lightning").unwrap().raw_value.as_deref(), Some("ln%20bc"));
        assert_eq!(uri.get("lno").unwrap().raw_value.as_deref(), Some("lno1"));
        assert_eq!(uri.get("pay").unwrap().raw_value.as_deref(), Some("pay1"));
        assert_eq!(uri.get("sp").unwrap().raw_value.as_deref(), Some("sp1"));
    }

    #[test]
    fn a_duplicate_is_reported_before_an_hrp_check() {
        // build() checks the non-repeat rule first, then HRP validity.
        let result = UriBuilder::new()
            .param("bc", BECH32_TESTNET)
            .label("a")
            .raw_param("label", "b")
            .build(bitcoin::Network::Bitcoin);
        assert!(matches!(result, Err(BuildError::DuplicateParameter(ref key)) if key == "label"));
    }

    #[test]
    fn built_output_reparses_to_identical_params() {
        let built = UriBuilder::new()
            .address(mainnet_body())
            .amount(bitcoin::Amount::from_sat(100_000))
            .label("Luke-Jr")
            .param("bc", BECH32_MAINNET)
            .param("lightning", "lnbc1invoice")
            .param("extra", "a b")
            .build(bitcoin::Network::Bitcoin)
            .unwrap();

        let reparsed = Uri::parse(&built.to_string())
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap();

        // Params are identical, in the same order and case.
        let built_params: Vec<_> = built.params().iter().map(|e| (e.key.clone(), e.raw_value.clone())).collect();
        let reparsed_params: Vec<_> = reparsed.params().iter().map(|e| (e.key.clone(), e.raw_value.clone())).collect();
        assert_eq!(built_params, reparsed_params);

        assert_eq!(built.to_string(), reparsed.to_string());
    }

    #[test]
    fn a_label_value_needing_encoding_round_trips_through_display_and_parse() {
        let built = UriBuilder::new()
            .address(mainnet_body())
            .label("Coffee & Pie")
            .build(bitcoin::Network::Bitcoin)
            .unwrap();
        let reparsed = Uri::parse(&built.to_string())
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap();
        // The '&' was encoded, so it did not split into a second parameter.
        assert_eq!(reparsed.params().len(), 1);
        assert_eq!(reparsed.label().unwrap().unwrap(), "Coffee & Pie");
    }

    #[test]
    fn an_extension_value_survives_round_trip_verbatim() {
        // The builder encodes `a+b c` to `a%2Bb%20c`; parsing keeps it encoded,
        // and `decoded_value` recovers the original on demand.
        let built = UriBuilder::new().param("lightning", "a+b c").build(bitcoin::Network::Bitcoin).unwrap();
        let reparsed = Uri::parse(&built.to_string())
            .unwrap()
            .require_network(bitcoin::Network::Bitcoin)
            .unwrap();
        let entry = reparsed.get("lightning").unwrap();
        assert_eq!(entry.raw_value.as_deref(), Some("a%2Bb%20c"));
        assert_eq!(entry.decoded_value().unwrap().unwrap(), "a+b c");
    }

    #[test]
    fn build_error_display_messages_are_distinct_and_mention_the_key() {
        let duplicate = BuildError::DuplicateParameter("label".to_string()).to_string();
        let invalid = BuildError::InvalidHrpValue { key: "bc".to_string() }.to_string();
        assert!(duplicate.contains("label"));
        assert!(invalid.contains("bc"));
        assert_ne!(duplicate, invalid);
    }

    #[test]
    fn a_param_value_that_is_not_utf8_once_decoded_is_an_access_error_not_a_build_error() {
        // raw_param passes bytes through; %ff is valid percent-encoding but
        // not UTF-8, so building succeeds and the failure only shows up when
        // something tries to decode it as text.
        let uri = UriBuilder::new().raw_param("label", "%ff").build(bitcoin::Network::Bitcoin).unwrap();
        assert!(matches!(uri.label(), Some(Err(AccessError::NotUtf8))));
    }

    #[test]
    fn build_network_checks_req_prefixed_hrp_params() {
        // `req-bc` must be network-checked like `bc`: a testnet value fails to
        // build rather than slipping through because of the `req-` prefix.
        let result = UriBuilder::new()
            .raw_param("req-bc", "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
            .build(bitcoin::Network::Bitcoin);
        assert!(result.is_err());
    }
}
