//! A lookup failure as the three columns every row that stores one uses: its
//! kind, the provider's HTTP status, and a diagnostic's detail.

use super::verdict_rows::unreadable;
use super::*;
use crate::signals::LookupFailure;

/// One failure as its three columns.
pub(super) struct FailureColumns {
    pub(super) kind: Option<&'static str>,
    pub(super) status: Option<i64>,
    pub(super) detail: Option<String>,
}

impl FailureColumns {
    const NONE: Self = Self {
        kind: None,
        status: None,
        detail: None,
    };
}

pub(super) fn failure_columns(failure: Option<&LookupFailure>) -> FailureColumns {
    match failure {
        None => FailureColumns::NONE,
        Some(LookupFailure::Network) => FailureColumns {
            kind: Some("network"),
            ..FailureColumns::NONE
        },
        Some(LookupFailure::Timeout) => FailureColumns {
            kind: Some("timeout"),
            ..FailureColumns::NONE
        },
        Some(LookupFailure::ArtworkAnalysis) => FailureColumns {
            kind: Some("artwork_analysis"),
            ..FailureColumns::NONE
        },
        Some(LookupFailure::Provider { status }) => FailureColumns {
            kind: Some("provider"),
            status: status.map(i64::from),
            detail: None,
        },
        Some(LookupFailure::Diagnostic { detail }) => FailureColumns {
            kind: Some("diagnostic"),
            status: None,
            detail: Some(detail.clone()),
        },
    }
}

pub(super) fn failure_of(
    kind: Option<String>,
    status: Option<i64>,
    detail: Option<String>,
) -> Result<Option<LookupFailure>, DbError> {
    let Some(kind) = kind else {
        return Ok(None);
    };
    Ok(Some(match kind.as_str() {
        "network" => LookupFailure::Network,
        "timeout" => LookupFailure::Timeout,
        "artwork_analysis" => LookupFailure::ArtworkAnalysis,
        "provider" => LookupFailure::Provider {
            status: status
                .map(|status| {
                    u16::try_from(status).map_err(|_| {
                        DbError::Message(format!("a stored provider status is {status}"))
                    })
                })
                .transpose()?,
        },
        "diagnostic" => LookupFailure::Diagnostic {
            detail: detail
                .ok_or_else(|| DbError::Message("a stored diagnostic states no detail".into()))?,
        },
        other => return Err(unreadable("lookup failure", other)),
    }))
}

