//! Why a step of identifying a folder did not answer: a catalog that could not
//! answer, or bae breaking on its own side.
//!
//! The two are told apart because they are resolved apart. A catalog that
//! failed to answer is a lookup error: the folder's answer is still out there,
//! and asking again may get it. bae breaking — a store read, the library
//! check, reading the folder's files — is an error of bae's own, which the
//! person sees with its text and which is logged where it happens.
//!
//! The locale never crosses the bridge: a catalog's failure crosses as its
//! typed reason and the UI renders the localized line, a provider's HTTP
//! status kept structured.

/// Why a catalog did not answer a lookup.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LookupFailure {
    /// A transport/connection failure that produced no HTTP response
    /// (connection refused, DNS failure, a dropped body).
    Network,
    /// An HTTP error response from the metadata provider. `status` is the
    /// HTTP status code when one was observed.
    Provider { status: Option<u16> },
    /// The request timed out before a response arrived.
    Timeout,
}

/// Something on bae's own side broke: a store read or write, the library
/// check, reading the folder's files, a document bae could not parse.
/// `detail` is the error chain, which the person is shown untranslated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InternalFailure {
    pub detail: String,
}

impl InternalFailure {
    /// The failure of `what` bae was doing, logged at error level where it
    /// happened.
    pub fn logged(what: &str, error: impl std::fmt::Display) -> Self {
        let detail = format!("{what}: {error}");
        tracing::error!("{detail}");
        Self { detail }
    }
}

/// Why a step that asks a catalog did not answer: the catalog could not, or
/// bae broke on its way there or back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Lookup(LookupFailure),
    Internal(InternalFailure),
}

impl From<LookupFailure> for Failure {
    fn from(failure: LookupFailure) -> Self {
        Self::Lookup(failure)
    }
}

impl From<InternalFailure> for Failure {
    fn from(failure: InternalFailure) -> Self {
        Self::Internal(failure)
    }
}
