//! Flat query-parameter access shared by every endpoint.
//!
//! Subsonic passes all parameters as query-string key/value pairs. This wraps
//! the parsed map with typed reads and the response-format resolution, so a
//! handler never re-parses `f`/`callback` or hand-rolls integer coercion.

use std::collections::HashMap;

use crate::envelope::Format;
use crate::error::SubError;

/// One request's query parameters.
pub(crate) struct Params(pub(crate) HashMap<String, String>);

impl Params {
    /// A parameter's value, or `None` when absent or empty. An empty value is
    /// treated as absent so `size=` reads like an omitted `size`.
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.0
            .get(name)
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// A required parameter, or error 10 when missing.
    pub(crate) fn require(&self, name: &'static str) -> Result<&str, SubError> {
        self.get(name).ok_or_else(|| SubError::missing_param(name))
    }

    /// An integer parameter, or `None` when absent. A present-but-unparseable
    /// value is a generic error rather than a silent default — the client asked
    /// for something the server can't honor.
    pub(crate) fn int(&self, name: &str) -> Result<Option<i64>, SubError> {
        match self.get(name) {
            None => Ok(None),
            Some(raw) => raw
                .parse::<i64>()
                .map(Some)
                .map_err(|_| SubError::generic(format!("parameter '{name}' must be an integer"))),
        }
    }

    /// An integer parameter with a fallback for the absent case.
    pub(crate) fn int_or(&self, name: &str, default: i64) -> Result<i64, SubError> {
        Ok(self.int(name)?.unwrap_or(default))
    }

    /// A boolean parameter (`true`/`false`), defaulting when absent.
    pub(crate) fn bool_or(&self, name: &str, default: bool) -> bool {
        match self.get(name) {
            Some("true") => true,
            Some("false") => false,
            _ => default,
        }
    }

    /// The response serialization this request asked for.
    pub(crate) fn format(&self) -> Format {
        Format::from_params(self.get("f"), self.get("callback"))
    }
}
