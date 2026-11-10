//! Reading extension parameters off a parsed URI.
//!
//! `bitcoin_uri` is a tokenizer: it parses the BIP 321 shape and hands back
//! every parameter still percent-encoded, without knowing what any of them
//! mean. An extension (payjoin's `pj`, a silent-payment `sp`, a BOLT 11
//! `lightning` invoice, anything else) reads whatever it needs straight off
//! the parsed [`Uri`] with [`Uri::get`]/[`Uri::get_all`] and decodes it with
//! [`ParamEntry::decoded_value`]. This example walks a body-less URI that
//! carries three unrelated extensions at once.
//!
//! Run with: `cargo run --example extension_params`

use bitcoin_uri::Uri;

fn main() {
    // A body-less URI (BIP 321 allows one): no on-chain address, just three
    // extension parameters, one of them marked `req-`.
    let input = "bitcoin:?sp=sp1qsilentpaymentaddress\
        &lightning=lnbc420bogusinvoice\
        &req-pj=https://example.com/pay%3Ftoken%3Dabc";

    let uri: Uri<_> = input.parse().expect("valid BIP 321 URI");

    // No body, so this is an extension-only payment request.
    assert!(uri.body().is_none());

    // A silent-payment wallet reads `sp` without the crate knowing what it is.
    if let Some(entry) = uri.get("sp") {
        let address = entry.decoded_value().expect("sp has a value").expect("valid UTF-8");
        println!("sp: {address}");
    }

    // `lightning` carries a BOLT 11 invoice (BOLT 12 offers use `lno`). The
    // value comes back in the case it arrived in, which is uppercase if it
    // came from a QR code.
    if let Some(entry) = uri.get("lightning") {
        let invoice = entry.decoded_value().expect("lightning has a value").expect("valid UTF-8");
        println!("lightning: {invoice}");
    }

    // `get` matches by base key, so it finds `req-pj` when asked for `pj`.
    // `decoded_value` applies RFC 3986 percent-decoding on demand, turning
    // `%3F` back into `?` and `%3D` into `=`.
    if let Some(entry) = uri.get("pj") {
        let endpoint = entry.decoded_value().expect("pj has a value").expect("valid UTF-8");
        println!("pj endpoint: {endpoint} (required: {})", entry.is_required());
    }

    // A consumer declares the extensions it understands; anything `req-` it
    // does not understand means it MUST reject the URI. Here it handles `pj`
    // and `sp` but not, say, a hypothetical `req-foo`.
    let understood = ["pj", "sp", "lightning"];
    let unsatisfied = uri.unsatisfied_requirements(&understood).count();
    assert_eq!(unsatisfied, 0, "every required extension is understood");
    println!("all {} parameters understood; safe to act on", uri.params().len());
}
