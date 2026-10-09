# bitcoin_uri Changelog

## Unreleased

### Breaking changes

- MSRV is 1.63, now declared in `rust-version`, and `bitcoin` must be at
  least 0.32.102. 1.63 matches `bitcoin-payment-instructions`, which this
  crate is meant to be a dependency of.
- The body address is now optional, so a body-less URI such as
  `bitcoin:?sp=...` parses. `Uri::body` returns `Option<&Address<V>>`.
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
- `AccessError`, returned when a value is read: its percent-decoded bytes
  may not be valid UTF-8.
- Parse-time `Error`, separate from `AccessError`. Invalid
  percent-encoding in any value fails the parse; a value that decodes to
  bytes that are not UTF-8 fails only when it is read.

## 0.1.0

- Fork from [Kixunil/bip21 @ `eae7201`](https://github.com/Kixunil/bip21/commit/eae72026cc5838bb169949641948b8c1cef99cbe) v0.5.0.
- Bump MSRV to 1.63.0 https://github.com/payjoin/bitcoin_uri/pull/1
- Fix escaping of '#' in parameter values and handling of unescaped '#' in uri https://github.com/payjoin/bitcoin_uri/pull/3
- Fix unicode on exactly index 7 must not panic https://github.com/payjoin/bitcoin_uri/pull/4
  - Make this fix rust 1.63.0 compatible https://github.com/payjoin/bitcoin_uri/pull/5
