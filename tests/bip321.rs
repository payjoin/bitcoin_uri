//! The example URIs from BIP 321's appendix, copied verbatim.
//!
//! The examples' body address, inherited from BIP 21, has a bad base58
//! checksum, so a parser that validates the body rejects every example that
//! uses it. The tests swap in a valid mainnet address before parsing.

use bitcoin::{Amount, Network};
use bitcoin_uri::{Error, Uri};

const BIP_ADDRESS: &str = "175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W";
const ADDRESS: &str = "1andreas3batLhQa2FawWjeyjCqyBzypd";

fn with_valid_address(example: &str) -> String {
    example.replace(BIP_ADDRESS, ADDRESS)
}

fn parse(example: &str) -> Uri<bitcoin::address::NetworkUnchecked> {
    let input = with_valid_address(example);
    input.parse().unwrap_or_else(|e| panic!("{} should parse: {:?}", input, e))
}

#[test]
fn the_bip_example_address_has_a_bad_checksum() {
    let error = BIP_ADDRESS.parse::<bitcoin::Address<_>>().unwrap_err();
    assert!(matches!(error, bitcoin::address::ParseError::Base58(_)), "{:?}", error);
    assert!(matches!(
        format!("bitcoin:{}", BIP_ADDRESS).parse::<Uri<_>>(),
        Err(Error::InvalidBodyAddress(_))
    ));
}

fn decoded(uri: &Uri<bitcoin::address::NetworkUnchecked>, key: &str) -> String {
    uri.get(key).unwrap().decoded_value().unwrap().unwrap()
}

#[test]
fn every_valid_example_parses_and_displays_as_written() {
    let mainnet = [
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?label=Luke-Jr",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=20.3&label=Luke-Jr",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=50&label=Luke-Jr&message=Donation%20for%20project%20xyz",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?lightning=lnbc420bogusinvoice",
        "bitcoin:?lightning=lnbc420bogusinvoice",
        "bitcoin:?lno=lno1bogusoffer",
        "bitcoin:?lno=lno1bogusoffer&sp=sp1qsilentpayment",
        "bitcoin:?sp=sp1qsilentpayment",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?sp=sp1qsilentpayment",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?req-somethingyoudontunderstand=50&req-somethingelseyoudontget=999",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?somethingyoudontunderstand=50&somethingelseyoudontget=999",
        "bitcoin:?bc=bc1qufgy354j3kmvuch987xe4s40836x3h0lg8f5n2&bc=bc1p5swkugezn97763tl0yty6556856uug0q6jflljvep9m4p7339x5qzyrh4g",
        "BITCOIN:BC1QUFGY354J3KMVUCH987XE4S40836X3H0LG8F5N2?BC=BC1P5SWKUGEZN97763TL0YTY6556856UUG0Q6JFLLJVEP9M4P7339X5QZYRH4G",
        "BITCOIN:?BC=BC1QUFGY354J3KMVUCH987XE4S40836X3H0LG8F5N2&BC=BC1P5SWKUGEZN97763TL0YTY6556856UUG0Q6JFLLJVEP9M4P7339X5QZYRH4G",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?pop=initiatingapp%3a",
        "bitcoin:?lightning=lnbc420bogusinvoice&pop=callbackuri%3abody%3fpop=",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?lightning=lnbc420bogusinvoice&pop=app%3a%3f",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?pop=https%3aiwantyouripaddress.com",
        "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?req-pop=https%3aevilwebsite.com",
    ];
    for example in mainnet {
        let uri = parse(example).require_network(Network::Bitcoin).unwrap();
        assert_eq!(uri.to_string(), with_valid_address(example));
    }

    let testnet = "bitcoin:?tb=tb1qghfhmd4zh7ncpmxl3qzhmq566jk8ckq4gafnmg";
    let uri = parse(testnet);
    assert!(!uri.is_valid_for_network(Network::Bitcoin));
    assert_eq!(uri.require_network(Network::Testnet).unwrap().to_string(), testnet);
}

#[test]
fn every_invalid_example_fails_to_parse() {
    let duplicates = [
        ("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?label=Luke-Jr&label=Matt", "label"),
        ("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=42&amount=10", "amount"),
        ("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=42&amount=42", "amount"),
        (
            "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?pop=callback%3a&req-pop=callback%3a",
            "req-pop",
        ),
    ];
    for (example, key) in duplicates {
        let input = with_valid_address(example);
        match input.parse::<Uri<_>>() {
            Err(Error::DuplicateParameter(ref duplicate)) => assert_eq!(duplicate, key),
            other => panic!("{} should be a duplicate {}, got {:?}", input, key, other),
        }
    }

    let input = "bitcoin:?bc=tb1qghfhmd4zh7ncpmxl3qzhmq566jk8ckq4gafnmg";
    assert!(matches!(input.parse::<Uri<_>>(), Err(Error::InvalidHrpValue { ref key }) if key == "bc"));
}

#[test]
fn the_examples_carry_the_values_they_describe() {
    let uri = parse("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=20.3&label=Luke-Jr");
    assert_eq!(uri.body().unwrap().clone().assume_checked().to_string(), ADDRESS);
    assert_eq!(uri.amount(), Some(Amount::from_sat(2_030_000_000)));
    assert_eq!(uri.label().unwrap().unwrap(), "Luke-Jr");

    let uri = parse("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?amount=50&label=Luke-Jr&message=Donation%20for%20project%20xyz");
    assert_eq!(uri.amount(), Some(Amount::from_int_btc(50)));
    assert_eq!(uri.message().unwrap().unwrap(), "Donation for project xyz");

    let uri = parse("bitcoin:?lno=lno1bogusoffer&sp=sp1qsilentpayment");
    assert!(uri.body().is_none());
    assert_eq!(decoded(&uri, "lno"), "lno1bogusoffer");
    assert_eq!(decoded(&uri, "sp"), "sp1qsilentpayment");

    let uri = parse("BITCOIN:?BC=BC1QUFGY354J3KMVUCH987XE4S40836X3H0LG8F5N2&BC=BC1P5SWKUGEZN97763TL0YTY6556856UUG0Q6JFLLJVEP9M4P7339X5QZYRH4G");
    assert_eq!(uri.get_all("bc").count(), 2);
}

#[test]
fn only_the_req_prefixed_unknown_params_are_unsatisfied() {
    let required = parse("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?req-somethingyoudontunderstand=50&req-somethingelseyoudontget=999");
    let unsatisfied: Vec<&str> = required.unsatisfied_requirements(&[]).map(|entry| entry.key.as_str()).collect();
    assert_eq!(unsatisfied, ["req-somethingyoudontunderstand", "req-somethingelseyoudontget"]);

    let optional = parse("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?somethingyoudontunderstand=50&somethingelseyoudontget=999");
    assert_eq!(optional.unsatisfied_requirements(&[]).count(), 0);

    // "the wallet MUST NOT make a payment" unless it understands pop.
    let uri = parse("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?req-pop=https%3aevilwebsite.com");
    assert_eq!(uri.unsatisfied_requirements(&[]).count(), 1);
    assert_eq!(uri.unsatisfied_requirements(&["pop"]).count(), 0);
}

#[test]
fn pop_callbacks_decode_to_the_uri_the_wallet_opens() {
    for (input, callback) in [
        ("bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?pop=initiatingapp%3a", "initiatingapp:"),
        (
            "bitcoin:?lightning=lnbc420bogusinvoice&pop=callbackuri%3abody%3fpop=",
            "callbackuri:body?pop=",
        ),
        (
            "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?lightning=lnbc420bogusinvoice&pop=app%3a%3f",
            "app:?",
        ),
        (
            "bitcoin:175tWpb8K1S7NmH4Zx6rewF9WQrcZv245W?pop=https%3aiwantyouripaddress.com",
            "https:iwantyouripaddress.com",
        ),
    ] {
        assert_eq!(parse(input).pop().unwrap().unwrap(), callback);
    }
}
