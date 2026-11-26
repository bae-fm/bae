//! Which crate owns each library table, from the `[[database.domains]]` rows
//! of `owner-construction-check.toml`, and the rule that a fragment reads only
//! its own crate's tables.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Policy {
    database: Database,
}

#[derive(Deserialize)]
struct Database {
    domains: Vec<Domain>,
}

#[derive(Deserialize)]
struct Domain {
    name: String,
    homes: Vec<String>,
    tables: Vec<String>,
}

/// Each library table with the crate that owns it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TableOwners {
    owners: BTreeMap<String, Owner>,
}

#[derive(Debug, PartialEq, Eq)]
struct Owner {
    crate_name: String,
    domain: String,
}

/// A call that reads a table another crate owns.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ForeignRead {
    pub(crate) table: String,
    pub(crate) owner: String,
    pub(crate) domain: String,
}

impl TableOwners {
    /// The owners the policy file's `text` names. Every home of a domain is
    /// in one crate, the crate's folder being the first part of the path.
    pub(crate) fn from_policy(text: &str) -> Result<Self, String> {
        let policy: Policy = toml::from_str(text).map_err(|error| error.to_string())?;
        let mut owners = BTreeMap::new();
        for domain in policy.database.domains {
            let crates: BTreeSet<&str> = domain
                .homes
                .iter()
                .map(|home| home.split('/').next().unwrap_or(home.as_str()))
                .collect();
            let Some(crate_name) = crates.first().filter(|_| crates.len() == 1) else {
                return Err(format!(
                    "the domain {:?} has homes in {} crates, and a domain belongs to one",
                    domain.name,
                    crates.len()
                ));
            };
            for table in &domain.tables {
                let owner =
                    Owner { crate_name: (*crate_name).to_owned(), domain: domain.name.clone() };
                if owners.insert(table.clone(), owner).is_some() {
                    return Err(format!("the table {table} is in two domains"));
                }
            }
        }
        Ok(Self { owners })
    }

    /// The tables of `tables`, lowercase, that a crate other than `crate_name`
    /// owns. A name no domain lists is no library table (coven's views, SQLite's
    /// own) and is free to read.
    pub(crate) fn foreign_reads(
        &self,
        crate_name: &str,
        tables: impl IntoIterator<Item = String>,
    ) -> Vec<ForeignRead> {
        tables
            .into_iter()
            .filter_map(|table| {
                let owner = self.owners.get(&table)?;
                (owner.crate_name != crate_name).then(|| ForeignRead {
                    table,
                    owner: owner.crate_name.clone(),
                    domain: owner.domain.clone(),
                })
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "domains_tests.rs"]
mod tests;
