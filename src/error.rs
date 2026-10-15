//! Parse-time errors for [`crate::Uri`].
//!
//! Only failures that must be known before a `Uri` can exist live here.
//! Anything else - including a non-UTF-8 or otherwise malformed value for a
//! param nobody asked for yet - is an access-time error on whichever typed
//! accessor needs it, not a parse error.

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
    /// The body address (before the `?`), or an HRP-keyed address parameter,
    /// failed to parse.
    InvalidBodyAddress(bitcoin::address::ParseError),
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
    /// `bc=`/`tb=` parameter, that does not belong to the requested network.
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
    /// The value's percent-encoding was invalid.
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
    /// A parameter key would not round-trip through a URI query string.
    InvalidKey(String),
    /// A raw parameter value would not round-trip through a URI query string.
    InvalidRawValue {
        /// The parameter key whose raw value is invalid.
        key: String,
    },
    /// A parameter BIP 321 forbids from repeating (`amount`, `label`,
    /// `message`, or `pop`) was added more than once - typically by also
    /// setting it through [`crate::UriBuilder::param`] or
    /// [`crate::UriBuilder::raw_param`].
    DuplicateParameter(String),
    /// A `bc=`/`tb=` HRP-keyed parameter's value is not a valid address of
    /// the network its key names, so the built URI would not parse back.
    InvalidHrpValue {
        /// The HRP parameter key, e.g. `"bc"`.
        key: String,
    },
    /// The body, or an HRP-keyed address, does not belong to the network
    /// passed to [`crate::UriBuilder::build`]. A checked URI never mixes
    /// networks, so building one with addresses on different networks fails
    /// here.
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
