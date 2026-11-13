//! A credit line as the apps read it: names with typed joins, which each app
//! words in the viewer's language (see `bae_types::credits`).

use bae_types::credits::{CreditJoin, CreditLine, CreditedName};

/// What follows a name in a credit line, other than the plain list
/// separator. The apps word each recognized join from the catalog message
/// `bridge_credit_join_key` names, and show `Unrecognized` as it is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeCreditJoin {
    Featuring,
    Ampersand,
    And,
    With,
    Versus,
    /// A join bae does not recognize, as its source wrote it.
    Unrecognized {
        text: String,
    },
}

/// One name of a credit line, and the join after it: `None` for the plain
/// list separator, and for the last name.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeCreditedName {
    pub name: String,
    pub join: Option<BridgeCreditJoin>,
}

/// A credit line. The apps join each run of names with no join between them
/// with their list formatter, and word the joins between runs.
#[derive(Debug, Clone, PartialEq, Eq, Default, uniffi::Record)]
pub struct BridgeCreditLine {
    pub names: Vec<BridgeCreditedName>,
}

impl BridgeCreditJoin {
    fn from_core(join: CreditJoin) -> Self {
        match join {
            CreditJoin::Featuring => Self::Featuring,
            CreditJoin::Ampersand => Self::Ampersand,
            CreditJoin::And => Self::And,
            CreditJoin::With => Self::With,
            CreditJoin::Versus => Self::Versus,
            CreditJoin::Written(text) => Self::Unrecognized { text },
        }
    }
}

impl BridgeCreditLine {
    pub(crate) fn from_core(line: CreditLine) -> Self {
        Self {
            names: line
                .names()
                .iter()
                .cloned()
                .map(|CreditedName { name, join }| BridgeCreditedName {
                    name,
                    join: join.map(BridgeCreditJoin::from_core),
                })
                .collect(),
        }
    }
}

/// The catalog message that words a recognized join, taking `{before}` and
/// `{after}`: the text on either side of it. `None` for a join written as
/// its source wrote it.
#[uniffi::export]
pub fn bridge_credit_join_key(join: BridgeCreditJoin) -> Option<String> {
    let word = match join {
        BridgeCreditJoin::Featuring => "featuring",
        BridgeCreditJoin::Ampersand => "ampersand",
        BridgeCreditJoin::And => "and",
        BridgeCreditJoin::With => "with",
        BridgeCreditJoin::Versus => "versus",
        BridgeCreditJoin::Unrecognized { .. } => return None,
    };
    Some(format!("core.credit.join.{word}"))
}
