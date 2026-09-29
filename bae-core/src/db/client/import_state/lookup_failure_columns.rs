//! A catalog's lookup failure as the two columns every row that stores one
//! uses: its kind, and the provider's HTTP status.

use super::verdict_rows::unreadable;
use super::*;
use crate::signals::LookupFailure;

/// One failure as its two columns.
pub(super) struct FailureColumns {
    pub(super) kind: Option<&'static str>,
    pub(super) status: Option<i64>,
}

pub(super) fn failure_columns(failure: Option<&LookupFailure>) -> FailureColumns {
    match failure {
        None => FailureColumns {
            kind: None,
            status: None,
        },
        Some(LookupFailure::Network) => FailureColumns {
            kind: Some("network"),
            status: None,
        },
        Some(LookupFailure::Timeout) => FailureColumns {
            kind: Some("timeout"),
            status: None,
        },
        Some(LookupFailure::Provider { status }) => FailureColumns {
            kind: Some("provider"),
            status: status.map(i64::from),
        },
    }
}

pub(super) fn failure_of(
    kind: Option<String>,
    status: Option<i64>,
) -> Result<Option<LookupFailure>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "network" => LookupFailure::Network,
        "timeout" => LookupFailure::Timeout,
        "provider" => LookupFailure::Provider {
            status: status
                .map(|status| {
                    u16::try_from(status).map_err(|_| {
                        DbError::Message(format!("a stored provider status is {status}"))
                    })
                })
                .transpose()?,
        },
        other => return Err(unreadable("lookup failure", other)),
    }))
}
