# bitcoin_uri Changelog

## Unreleased

### Breaking changes

- MSRV is now 1.85, and the `bitcoin` dependency is 0.32.102, to track
  rust-payjoin.
- `Uri` is now owned and has no lifetime parameter: `Uri<V>` instead of
  `Uri<'a, NetVal, Extras>`.
- Removed the `Extras` type parameter and the `DeserializeParams`,
  `DeserializationState`, `SerializeParams`, and `Param` traits/types. An
  extension now reads its parameters straight off the parsed `Uri` via the
  `Params` multimap instead of plugging a type into the parser.
- Removed the `non-compliant-bytes` feature and the optional `either`
  dependency. A non-UTF-8 parameter value is now an access-time error on the
  typed accessor that needs it, not a separate API.

### Added

- `Params`/`ParamEntry`: an ordered, case-insensitive multimap of the raw
  query parameters, preserving source order and percent-encoding, with
  `ParamEntry::decoded_value` for on-demand RFC 3986 decoding.
- Parse-time `Error` and access-time `AccessError`, kept separate: a `Uri`
  parses as long as it is structurally valid, and decoding an individual
  value can fail later without failing the parse.

## 0.1.0

- Fork from [Kixunil/bip21 @ `eae7201`](https://github.com/Kixunil/bip21/commit/eae72026cc5838bb169949641948b8c1cef99cbe) v0.5.0.
- Bump MSRV to 1.63.0 https://github.com/payjoin/bitcoin_uri/pull/1
- Fix escaping of '#' in parameter values and handling of unescaped '#' in uri https://github.com/payjoin/bitcoin_uri/pull/3
- Fix unicode on exactly index 7 must not panic https://github.com/payjoin/bitcoin_uri/pull/4
  - Make this fix rust 1.63.0 compatible https://github.com/payjoin/bitcoin_uri/pull/5
