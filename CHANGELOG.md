# bitcoin_uri Changelog

## Unreleased

### Breaking changes

- MSRV is now 1.85, and the `bitcoin` dependency is 0.32.102, to track
  rust-payjoin.

## 0.1.0

- Fork from [Kixunil/bip21 @ `eae7201`](https://github.com/Kixunil/bip21/commit/eae72026cc5838bb169949641948b8c1cef99cbe) v0.5.0.
- Bump MSRV to 1.63.0 https://github.com/payjoin/bitcoin_uri/pull/1
- Fix escaping of '#' in parameter values and handling of unescaped '#' in uri https://github.com/payjoin/bitcoin_uri/pull/3
- Fix unicode on exactly index 7 must not panic https://github.com/payjoin/bitcoin_uri/pull/4
  - Make this fix rust 1.63.0 compatible https://github.com/payjoin/bitcoin_uri/pull/5
