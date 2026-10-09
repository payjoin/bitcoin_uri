//! Access-time errors for decoding a parameter value.
//!
//! A parameter can be structurally fine and still fail to decode later: its
//! percent-encoding may be invalid, or the decoded bytes may not be UTF-8.
//! That failure is reported here, at the point a typed, decoded view of the
//! value is asked for, rather than at parse time.

use core::fmt;
use percent_encoding_rfc3986::PercentDecodeError;

/// Why decoding an already-parsed parameter's value failed.
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
