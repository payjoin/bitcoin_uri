# Rust implementation of BIP 321 bitcoin URIs

Rust-idiomatic, compliant, flexible and performant bitcoin URI crate.

## About

**Important:** this is the 0.2 line, a rewrite against
[issue #12](https://github.com/payjoin/bitcoin_uri/issues/12). Its API is not
yet stable. 0.1 remains the published, stable crate.

* Rust-idiomatic: owned types, no lifetime parameter.
* Compliant: parses the BIP 321 shape.
* Flexible: an extension reads whatever parameters it needs straight off the
            parsed `Uri`; this crate never needs to know what they mean.
* Performant: values stay percent-encoded until something decodes them.

The crate is `no_std` but does require `alloc`.

## Features

* `std` enables integration with `std` - mainly `std::error::Error`.

## MSRV

1.63

## License

MITNFA
