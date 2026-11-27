//! A catalog's lookup failure as the two columns every row that stores one
//! uses: its kind, and the provider's HTTP status.

use super::*;
use bae_catalogs::LookupFailure;

/// One failure as its two columns.
pub(super) struct FailureColumns {
    pub(super) kind: Option<&'static str>,
    pub(super) status: Option<i64>,
}

pub(super) fn failure_columns(failure: Option<&LookupFailure>) -> FailureColumns {
    match failure.map(failure_stated) {
        None => FailureColumns { kind: None, status: None },
        Some((kind, status)) => FailureColumns { kind: Some(kind), status },
    }
}

/// The two columns of a row that always states `failure`: its kind and the
/// provider's HTTP status.
pub(super) fn failure_stated(failure: &LookupFailure) -> (&'static str, Option<i64>) {
    match failure {
        LookupFailure::Network => ("network", None),
        LookupFailure::Timeout => ("timeout", None),
        LookupFailure::Provider { status } => ("provider", status.map(i64::from)),
    }
}

pub(super) fn failure_of(
    kind: Option<String>,
    status: Option<i64>,
) -> Result<Option<LookupFailure>, DbError> {
    kind.map(|kind| failure_named(&kind, status)).transpose()
}

/// The failure of kind `kind`, with the provider's `status` when it has one.
pub(super) fn failure_named(kind: &str, status: Option<i64>) -> Result<LookupFailure, DbError> {
    Ok(match kind {
        "network" => LookupFailure::Network,
        "timeout" => LookupFailure::Timeout,
        "provider" => LookupFailure::Provider {
            status: status
                .map(|status| {
                    u16::try_from(status).map_err(|_out_of_range| {
                        DbError::unreadable("a stored provider status", status)
                    })
                })
                .transpose()?,
        },
        other => return Err(DbError::unreadable("a stored lookup failure's kind", other)),
    })
}
