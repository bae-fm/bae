//! The Subsonic error taxonomy this server uses.
//!
//! Subsonic reports failures inside a normal `<subsonic-response>` with
//! `status="failed"` and an `<error code=".." message=".."/>` — never an HTTP
//! error status. The codes here are the subset the browse+play surface needs;
//! the full list is in the Subsonic API doc.

/// A Subsonic error: a numeric code plus a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubError {
    pub(crate) code: u32,
    pub(crate) message: String,
}

impl SubError {
    /// 10 — a required parameter is missing.
    pub(crate) fn missing_param(name: &str) -> Self {
        Self {
            code: 10,
            message: format!("Required parameter '{name}' is missing"),
        }
    }

    /// 40 — wrong username or password.
    pub(crate) fn wrong_credentials() -> Self {
        Self {
            code: 40,
            message: "Wrong username or password".to_string(),
        }
    }

    /// 70 — the requested data was not found.
    pub(crate) fn not_found() -> Self {
        Self {
            code: 70,
            message: "The requested data was not found".to_string(),
        }
    }

    /// 0 — a generic error, message carrying the detail.
    pub(crate) fn generic(message: impl Into<String>) -> Self {
        Self {
            code: 0,
            message: message.into(),
        }
    }
}
