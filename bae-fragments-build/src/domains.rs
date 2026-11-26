//! The database domains, from the `[[database.domains]]` rows of
//! `owner-construction-check.toml`: the files each domain's homes hold, the
//! tables each owns, and the rule that a call reads only the tables of the
//! domain that holds its file.

use serde::Deserialize;
use std::collections::BTreeMap;

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

/// Each domain's homes, and each library table with the domain that owns it.
#[derive(Debug)]
pub(crate) struct Domains {
    /// Each home with its domain's name. No two domains' homes hold one file.
    homes: Vec<(String, String)>,
    /// Each table with its domain's name.
    tables: BTreeMap<String, String>,
}

/// A table a call reads whose domain's homes don't hold the call's file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ForeignRead {
    pub(crate) table: String,
    pub(crate) domain: String,
}

/// Whether the home `home`, a file or a folder whose path ends in `/`, holds
/// `path`.
fn holds(home: &str, path: &str) -> bool {
    path == home || (home.ends_with('/') && path.starts_with(home))
}

impl Domains {
    /// The domains the policy file's `text` names.
    pub(crate) fn from_policy(text: &str) -> Result<Self, String> {
        let policy: Policy = toml::from_str(text).map_err(|error| error.to_string())?;
        let mut homes: Vec<(String, String)> = Vec::new();
        let mut tables = BTreeMap::new();
        for domain in policy.database.domains {
            for home in domain.homes {
                if let Some((other_home, other)) = homes.iter().find(|(other_home, other)| {
                    *other != domain.name && (holds(other_home, &home) || holds(&home, other_home))
                }) {
                    return Err(format!(
                        "the home {home} of the domain {:?} and the home {other_home} of the \
                         domain {other:?} hold the same files, and a file is in one domain",
                        domain.name
                    ));
                }
                homes.push((home, domain.name.clone()));
            }
            for table in domain.tables {
                if tables.insert(table.clone(), domain.name.clone()).is_some() {
                    return Err(format!("the table {table} is in two domains"));
                }
            }
        }
        Ok(Self { homes, tables })
    }

    /// The tables of `tables`, lowercase, that the file `file`, a path in the
    /// workspace, may not read: those of a domain whose homes don't hold it. A
    /// name no domain lists is no library table (coven's views, SQLite's own)
    /// and is free to read.
    pub(crate) fn foreign_reads(
        &self,
        file: &str,
        tables: impl IntoIterator<Item = String>,
    ) -> Vec<ForeignRead> {
        let file_domain =
            self.homes.iter().find(|(home, _)| holds(home, file)).map(|(_, domain)| domain);
        tables
            .into_iter()
            .filter_map(|table| {
                let domain = self.tables.get(&table)?;
                (file_domain != Some(domain)).then(|| ForeignRead { table, domain: domain.clone() })
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "domains_tests.rs"]
mod tests;
