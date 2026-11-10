//! The placement's shared test fixtures: a placement row's columns, the
//! rows a test expects, and what a write did to the placement. The tests of
//! each file that keeps the placement read through them: each write to a
//! table the placement is kept from moves exactly the placement rows it
//! should, in that write.

use crate::tables::import_list::tests::*;
use std::collections::BTreeMap;

/// A placement row's columns, past its key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placed {
    pub(crate) root_position: i64,
    pub(crate) tab: String,
    pub(crate) pending_state: Option<String>,
    pub(crate) needs_you_reason: Option<String>,
    pub(crate) group_name: Option<String>,
    pub(crate) own_date: Option<i64>,
    pub(crate) unit_date: Option<i64>,
    pub(crate) display_path: Option<String>,
    pub(crate) shown_title: Option<String>,
    pub(crate) shown_artists: Option<String>,
}

/// A placement row's key: its watched folder, what it is, and the release's
/// key or the group's folder.
pub(crate) type Item = (String, &'static str, String);

/// What a write did to the placement: each row it changed, as it now stands,
/// or `None` for one it took out.
pub(crate) type Moved = BTreeMap<Item, Option<Placed>>;

/// When the scans first saw a folder no filesystem date dates: the time of
/// the test store's clock, in milliseconds.
pub(crate) fn seen() -> i64 {
    bae_store::test_fixtures::fixed_now().timestamp_millis()
}

/// A release on Found nobody has looked up, as its scan alone places it:
/// dated by when the scan first saw its folder.
pub(crate) fn found(display_path: &str) -> Placed {
    Placed {
        root_position: 0,
        tab: "pending".to_string(),
        pending_state: Some("not_looked_up".to_string()),
        needs_you_reason: None,
        group_name: None,
        own_date: Some(seen()),
        unit_date: Some(seen()),
        display_path: Some(display_path.to_string()),
        shown_title: Some(last_component(display_path).to_string()),
        shown_artists: None,
    }
}

/// A folder that failed validation, which only Skipped lists.
pub(crate) fn failed_validation(display_path: &str) -> Placed {
    Placed { tab: "skipped".to_string(), pending_state: None, ..found(display_path) }
}

/// The header of the group at `name`, dated `date`.
pub(crate) fn header(name: &str, date: Option<i64>) -> Placed {
    Placed {
        root_position: 0,
        tab: "pending".to_string(),
        pending_state: None,
        needs_you_reason: None,
        group_name: Some(name.to_string()),
        own_date: None,
        unit_date: date,
        display_path: None,
        shown_title: None,
        shown_artists: None,
    }
}

fn last_component(display_path: &str) -> &str {
    display_path.rsplit('/').next().unwrap_or(display_path)
}

impl Placed {
    /// On `tab`, where only Found holds a state.
    pub(crate) fn on(self, tab: &str) -> Self {
        Self {
            tab: tab.to_string(),
            pending_state: (tab == "pending").then(|| "not_looked_up".to_string()),
            needs_you_reason: None,
            ..self
        }
    }

    pub(crate) fn state(self, state: &str) -> Self {
        Self { pending_state: Some(state.to_string()), needs_you_reason: None, ..self }
    }

    /// Needing the person, for `reason`.
    pub(crate) fn needs_you(self, reason: &str) -> Self {
        Self {
            pending_state: Some("needs_you".to_string()),
            needs_you_reason: Some(reason.to_string()),
            ..self
        }
    }

    /// Dated `date` and sorting by it.
    pub(crate) fn dated(self, date: i64) -> Self {
        Self { own_date: Some(date), unit_date: Some(date), ..self }
    }

    /// Listed under the group at `name`, sorting in its header's place.
    pub(crate) fn grouped(self, name: &str) -> Self {
        Self { group_name: Some(name.to_string()), unit_date: None, ..self }
    }

    pub(crate) fn shown(self, title: &str, artists: Option<&str>) -> Self {
        Self {
            shown_title: Some(title.to_string()),
            shown_artists: artists.map(str::to_string),
            ..self
        }
    }
}

impl Library {
    pub(crate) fn release(&self, display_path: &str) -> Item {
        (self.root_text(), "candidate", self.text(display_path))
    }

    /// The release at the folder `relative` below the watched folder. Only the
    /// Unix test of a folder name that isn't valid UTF-8 spells a release by
    /// its path.
    #[cfg(unix)]
    pub(crate) fn release_at(&self, relative: &std::path::Path) -> Item {
        let folder = crate::tables::watched_path::folder_below(
            &self.root,
            &crate::RelativeFolder::of(relative).unwrap(),
        );
        (self.root_text(), "candidate", coven::path_text(&folder))
    }

    /// The release the grouping `grouping:{id}` reads.
    pub(crate) fn grouped_release(&self, id: &str) -> Item {
        (self.root_text(), "candidate", format!("grouping:{id}"))
    }

    pub(crate) fn invalid(&self, display_path: &str) -> Item {
        (self.root_text(), "invalid", self.text(display_path))
    }

    pub(crate) fn group(&self, name: &str) -> Item {
        (self.root_text(), "group", name.to_string())
    }

    fn root_text(&self) -> String {
        coven::path_text(&self.root)
    }

    /// The text of the folder at `display_path`, as a placement row's key
    /// reads back.
    pub(crate) fn text(&self, display_path: &str) -> String {
        coven::path_text(&self.path(display_path))
    }

    /// Every placement row as it stands.
    pub(crate) async fn placed(&self) -> BTreeMap<Item, Placed> {
        self.db
            .read(|sql| {
                let rows = sql.query(
                    "SELECT coven_path_text(watched_folder_path), item, coven_path_text(key),
                            root_position, tab, pending_state,
                            needs_you_reason, group_name, own_date, unit_date, display_path,
                            shown_title, shown_artists
                     FROM import_list_placement",
                    [],
                    |row| {
                        let item = match row.get::<_, String>(1)?.as_str() {
                            "candidate" => "candidate",
                            "invalid" => "invalid",
                            "group" => "group",
                            other => panic!("placement row of unknown item {other}"),
                        };
                        Ok((
                            (row.get::<_, String>(0)?, item, row.get::<_, String>(2)?),
                            Placed {
                                root_position: row.get(3)?,
                                tab: row.get(4)?,
                                pending_state: row.get(5)?,
                                needs_you_reason: row.get(6)?,
                                group_name: row.get(7)?,
                                own_date: row.get(8)?,
                                unit_date: row.get(9)?,
                                display_path: row.get(10)?,
                                shown_title: row.get(11)?,
                                shown_artists: row.get(12)?,
                            },
                        ))
                    },
                )?;
                Ok(rows.into_iter().collect())
            })
            .await
            .unwrap()
    }

    /// What running `write` did to the placement.
    pub(crate) async fn moved(&self, write: Write) -> Moved {
        self.moved_after(self.run(write)).await
    }

    /// What `change`, a write this library's own writes can't make, such as
    /// another device's arriving, did to the placement.
    pub(crate) async fn moved_after(&self, change: impl std::future::Future<Output = ()>) -> Moved {
        let before = self.placed().await;
        change.await;
        self.searchable().await;
        let after = self.placed().await;
        before
            .keys()
            .chain(after.keys())
            .filter(|item| before.get(*item) != after.get(*item))
            .map(|item| (item.clone(), after.get(item).cloned()))
            .collect()
    }
}

/// The moves `rows` name, as a test spells them.
pub(crate) fn moves<const N: usize>(rows: [(Item, Option<Placed>); N]) -> Moved {
    rows.into_iter().collect()
}
