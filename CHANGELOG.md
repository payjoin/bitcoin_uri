# bitcoin_uri Changelog

## 0.2.0

0.2 parses BIP 321, the successor to BIP 21, and replaces the 0.1 API. `Uri`
is owned, an extension reads its parameters off the parsed URI instead of
plugging an `Extras` type into the parser, and `UriBuilder` assembles new
URIs.

### Breaking changes

- A URI with a `req-` parameter the caller does not understand now parses.
  0.1 rejected it at parse time. Pass the keys you handle to
  `Uri::unsatisfied_requirements` and reject the URI if it returns anything.
- `Uri` is owned and has no lifetime parameter: `Uri<V>` instead of
  `Uri<'a, NetVal, Extras>`.
- `Uri`'s public fields, `Uri::new` and `Uri::with_extras` are gone. Read a
  parsed URI with `body`, `amount`, `label`, `message`, `pop`, `get` and
  `get_all`, and assemble one with `UriBuilder`.
- The body address is optional, so a body-less URI such as
  `bitcoin:?sp=...` parses. `Uri::body` returns `Option<&Address<V>>`.
- Removed the `de` and `ser` modules along with `DeserializeParams`,
  `DeserializationState`, `DeserializationError`, `SerializeParams`,
  `Param`, `ParamKind` and `NoExtras`. Parse failures are a single `Error`
  enum instead of `de::Error` and `UriError`.
- Removed the `non-compliant-bytes` feature and the optional `either`
  dependency. A value whose decoded bytes are not UTF-8 fails with
  `AccessError` when an accessor reads it, not at parse time.
- Parameter keys match case-insensitively, as BIP 321 requires, so
  `LABEL=` is the label.
- A `bc`/`tb`/`bcrt` parameter must be a segwit address of the network its
  key names, or parsing fails with `Error::InvalidHrpValue`.
- `Uri::require_network` checks those parameters as well as the body, and
  fails with `NetworkError`.
- `Display` of a parsed `Uri` writes back the exact string it was parsed
  from. `{:#}` still writes the QR form, with an uppercase body.
- `bitcoin` must be at least 0.32.102. The MSRV is still 1.63 and is now
  declared in `rust-version`.

### Added

- `Params` and `ParamEntry`, the query parameters in source order and
  still percent-encoded. `ParamEntry::decoded_value` decodes a value on
  demand.
- `Uri::unsatisfied_requirements`, `ParamEntry::requirement` and
  `ParamEntry::is_required`.
- `Uri::is_valid_for_network`, `Uri::pop` and `Uri::source_str`.
- `UriBuilder` and `BuildError`. A built URI parses back to the same
  parameters. Keys and raw values must be RFC 3986 query characters, keys
  may not contain `%`, and `build` rejects a URI that mixes networks.
- An `extension_params` example that reads `pj`, `sp` and `lightning` off a
  parsed URI.

## 0.1.0

- Fork from [Kixunil/bip21 @ `eae7201`](https://github.com/Kixunil/bip21/commit/eae72026cc5838bb169949641948b8c1cef99cbe) v0.5.0.
- Bump MSRV to 1.63.0 https://github.com/payjoin/bitcoin_uri/pull/1
- Fix escaping of '#' in parameter values and handling of unescaped '#' in uri https://github.com/payjoin/bitcoin_uri/pull/3
- Fix unicode on exactly index 7 must not panic https://github.com/payjoin/bitcoin_uri/pull/4
  - Make this fix rust 1.63.0 compatible https://github.com/payjoin/bitcoin_uri/pull/5
