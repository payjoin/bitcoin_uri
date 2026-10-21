//! The crate's error types.
//!
//! A parse [`Error`] means there is no `Uri`. A value whose percent-decoded
//! bytes are not UTF-8 is an [`AccessError`] from whichever accessor reads
//! it, not a parse error.

use alloc::borrow::ToOwned;
use alloc::string::String;
use core::fmt;
use percent_encoding_rfc3986::PercentDecodeError;

/// Why parsing a `bitcoin:` URI into a [`crate::Uri`] failed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Error {
    /// The string does not start with the `bitcoin:` scheme
    /// (case-insensitively).
    InvalidScheme,
    /// The body address (before the `?`) failed to parse.
    InvalidBodyAddress(bitcoin::address::ParseError),
    /// A `bc`, `tb` or `bcrt` parameter's value is not a segwit address of
    /// the network its key names.
    InvalidHrpValue {
        /// The parameter key as it appeared, e.g. `"req-bc"`.
        key: String,
    },
    /// The `amount` parameter failed to parse.
    InvalidAmount(bitcoin::amount::ParseAmountError),
    /// A `label`, `message`, `pop`, or `amount` parameter appeared more than
    /// once. BIP 321 forbids repeating any of these four.
    DuplicateParameter(String),
    /// A percent-encoded parameter value contained invalid percent-encoding.
    InvalidPercentEncoding {
        /// The parameter key whose value failed to decode.
        key: String,
        /// The underlying decode failure.
        error: PercentDecodeError,
    },
}

impl Error {
    pub(crate) fn percent_decode(key: &str) -> impl '_ + FnOnce(PercentDecodeError) -> Self {
        move |error| Error::InvalidPercentEncoding { key: key.to_owned(), error }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidScheme => write!(f, "the URI has an invalid scheme"),
            Error::InvalidBodyAddress(_) => write!(f, "the URI contains an invalid address"),
            Error::InvalidHrpValue { key } => {
                write!(f, "the '{}' parameter's value is not a segwit address for its network", key)
            },
            Error::InvalidAmount(_) => write!(f, "the amount parameter is invalid"),
            Error::DuplicateParameter(key) => write!(f, "the URI contains a duplicate '{}' parameter", key),
            Error::InvalidPercentEncoding { key, .. } => write!(f, "can not percent-decode parameter '{}'", key),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::InvalidScheme => None,
            Error::InvalidBodyAddress(error) => Some(error),
            Error::InvalidHrpValue { .. } => None,
            Error::InvalidAmount(error) => Some(error),
            Error::DuplicateParameter(_) => None,
            Error::InvalidPercentEncoding { error, .. } => Some(error),
        }
    }
}

/// Why network validation failed after parsing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum NetworkError {
    /// [`crate::Uri::require_network`] found the body address, or a
    /// `bc=`/`tb=`/`bcrt=` parameter, that does not belong to the requested
    /// network.
    NetworkMismatch,
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::NetworkMismatch => write!(f, "the URI is not valid for the requested network"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for NetworkError {}

/// Why decoding an already-parsed parameter's value failed.
///
/// This is distinct from [`Error`]: the parameter itself parsed fine (it is
/// right there in [`crate::Uri::params`]), it is only the typed, decoded
/// view of it - [`crate::Uri::label`], [`crate::Uri::message`],
/// [`crate::Uri::pop`] - that failed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AccessError {
    /// The value's percent-encoding was invalid. Parsing already rejects
    /// such a value, so only a hand-built [`crate::ParamEntry`] gets here.
    PercentDecode(PercentDecodeError),
    /// The value decoded to bytes that are not valid UTF-8.
    NotUtf8,
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccessError::PercentDecode(_) => write!(f, "invalid percent-encoding"),
            AccessError::NotUtf8 => write!(f, "value is not valid UTF-8"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for AccessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AccessError::PercentDecode(error) => Some(error),
            AccessError::NotUtf8 => None,
        }
    }
}

/// Why [`crate::UriBuilder::build`] could not produce a valid URI.
///
/// Distinct from [`Error`], which is about parsing a string: these are the
/// ways a `Uri` assembled from typed parts can still be ill-formed.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BuildError {
    /// A parameter key is empty, is a bare `req-`, is not made of RFC 3986
    /// query characters other than `&` and `=`, or has an invalid `%` escape.
    InvalidKey(String),
    /// A raw parameter value is not made of RFC 3986 query characters other
    /// than `&` and `=`, or has an invalid `%` escape.
    InvalidRawValue {
        /// The parameter key whose raw value is invalid.
        key: String,
    },
    /// A parameter BIP 321 forbids from repeating (`amount`, `label`,
    /// `message`, or `pop`) was added more than once - typically by also
    /// setting it through [`crate::UriBuilder::param`] or
    /// [`crate::UriBuilder::raw_param`].
    DuplicateParameter(String),
    /// A `bc`, `tb` or `bcrt` parameter's value is not a segwit address of the
    /// network its key names, so the built URI would not parse back.
    InvalidHrpValue {
        /// The HRP parameter key, e.g. `"bc"`.
        key: String,
    },
    /// The HRP-keyed parameters name different networks, or the body does not
    /// belong to the network they name. A checked URI never mixes networks.
    Network(NetworkError),
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::InvalidKey(key) => write!(f, "the '{}' parameter key is invalid", key),
            BuildError::InvalidRawValue { key } => write!(f, "the '{}' parameter's raw value is invalid", key),
            BuildError::DuplicateParameter(key) => {
                write!(f, "the '{}' parameter was set more than once", key)
            },
            BuildError::InvalidHrpValue { key } => {
                write!(f, "the '{}' parameter's value is not a valid address for its network", key)
            },
            BuildError::Network(error) => write!(f, "{}", error),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BuildError::Network(error) => Some(error),
            _ => None,
        }
    }
}
