# Rust implementation of BIP 321 bitcoin URIs

Rust-idiomatic, compliant, flexible and performant bitcoin URI crate.

## About

0.2 replaces the 0.1 API. [CHANGELOG.md](CHANGELOG.md) lists what changed
for a 0.1 user.

* Rust-idiomatic: owned types, no lifetime parameter.
* Compliant: parses the full BIP 321 shape, including a body-less URI, and
             reports `req-` parameters a client does not understand through
             `Uri::unsatisfied_requirements` rather than enforcing them itself.
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
