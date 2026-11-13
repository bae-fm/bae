//! A credit's role as the apps word it: a label for the role and one for
//! each qualifier, from the catalog for what bae names and as the catalog
//! wrote it for the rest.

use super::pressing::{localized, verbatim};
use super::BridgeTermLabel;
use bae_types::credit_role::{
    CreditRole, CreditRoleKind, CreditedAs, QualifierKind, RoleQualifier,
};

/// What an artist did, worded in parts: the role, then its qualifiers in the
/// order the catalog stated them. The apps join the qualifiers with their list
/// formatter and put them in parentheses after the role
/// (`core.credit.qualified`).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCreditRole {
    pub role: BridgeTermLabel,
    pub qualifiers: Vec<BridgeTermLabel>,
}

/// The catalog key naming a role bae names.
pub(crate) fn credit_role_key(kind: CreditRoleKind) -> String {
    format!("core.credit.role.{}", kind.key())
}

/// The catalog key naming a qualifier bae names.
pub(crate) fn credit_qualifier_key(kind: QualifierKind) -> String {
    format!("core.credit.qualifier.{}", kind.key())
}

/// The catalog key naming a performer of no part the catalog names.
pub(crate) const PERFORMER_KEY: &str = "core.credit.role.performer";

impl BridgeCreditRole {
    pub(crate) fn from_core(credited: CreditedAs) -> Self {
        let CreditedAs { role, qualifiers } = credited;
        Self {
            role: match role {
                CreditRole::Kind(kind) => localized(&credit_role_key(kind)),
                CreditRole::Performer(None) => localized(PERFORMER_KEY),
                CreditRole::Performer(Some(part)) => verbatim(&part),
                CreditRole::Stated(words) => verbatim(&words),
            },
            qualifiers: qualifiers
                .into_iter()
                .map(|qualifier| match qualifier {
                    RoleQualifier::Kind(kind) => localized(&credit_qualifier_key(kind)),
                    RoleQualifier::Stated(words) => verbatim(&words),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
#[path = "credit_role_tests.rs"]
mod tests;
