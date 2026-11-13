//! Which rows of a Discogs tracklist a release credit's `tracks` names.
//!
//! A release-level credit may state `tracks`: a comma-separated list of
//! items, each a position (`3`, `A1`, `50-1`) or a range of two, `X to Y`,
//! which names the rows from `X` through `Y` in tracklist order. Positions
//! compare as written, with case aside. A row's sub-tracks are part of it: a
//! position names its sub-tracks too, and a range ends after its last row's.
//! Spaces and empty items are ignored (`14-11,14-13`, `4, 5, `).

use crate::discogs::{DiscogsRoleArtist, DiscogsTrack};
use tracing::warn;

/// Where a row sits in a tracklist: its index at the top level, then in
/// each row's sub-tracks.
pub(crate) type RowPath = Vec<usize>;

/// A row in reading order.
struct Row {
    path: RowPath,
    position: String,
    /// How many rows, itself and its sub-tracks included, it spans.
    span: usize,
}

fn flatten(tracklist: &[DiscogsTrack], parent: &[usize], rows: &mut Vec<Row>) {
    for (index, row) in tracklist.iter().enumerate() {
        let mut path = parent.to_vec();
        path.push(index);
        let at = rows.len();
        rows.push(Row { path: path.clone(), position: row.position.trim().to_string(), span: 1 });
        flatten(&row.sub_tracks, &path, rows);
        rows[at].span = rows.len() - at;
    }
}

/// The rows `tracks` names, in tracklist order, and the items that name no
/// row.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Named {
    pub(crate) rows: Vec<RowPath>,
    pub(crate) unnamed: Vec<String>,
}

/// The rows of `tracklist` that `tracks` names.
pub(crate) fn named_rows(tracklist: &[DiscogsTrack], tracks: &str) -> Named {
    let mut flat = Vec::new();
    flatten(tracklist, &[], &mut flat);
    let find = |position: &str| {
        let position = position.trim();
        (!position.is_empty())
            .then(|| flat.iter().position(|row| row.position.eq_ignore_ascii_case(position)))
            .flatten()
    };
    let mut covered = vec![false; flat.len()];
    let mut unnamed = Vec::new();
    for item in tracks.split(',').map(str::trim).filter(|item| !item.is_empty()) {
        let (first, last) = match split_range(item) {
            Some((first, last)) => (find(first), find(last)),
            None => (find(item), find(item)),
        };
        match (first, last) {
            (Some(first), Some(last)) if first <= last => {
                covered[first..last + flat[last].span].fill(true);
            }
            _ => unnamed.push(item.to_string()),
        }
    }
    Named {
        rows: flat
            .into_iter()
            .zip(covered)
            .filter(|(_, covered)| *covered)
            .map(|(row, _)| row.path)
            .collect(),
        unnamed,
    }
}

/// The two ends of an `X to Y` item.
fn split_range(item: &str) -> Option<(&str, &str)> {
    let lowered = item.to_ascii_lowercase();
    let at = lowered.find(" to ")?;
    Some((&item[..at], &item[at + 4..]))
}

/// `tracklist` with each credit of `credits` that states `tracks` added to the
/// rows it names, and those credits left out of the credits returned: the
/// release's own. A credit none of whose items names a row is dropped, with
/// a warning naming the release; an item that names no row of a credit that
/// names others is skipped, with one.
pub(crate) fn scoped(
    release_id: &str,
    tracklist: &[DiscogsTrack],
    credits: &[DiscogsRoleArtist],
) -> Vec<DiscogsTrack> {
    let mut scoped = tracklist.to_vec();
    for credit in credits {
        let Some(tracks) = credit.tracks.as_deref() else {
            continue;
        };
        let named = named_rows(tracklist, tracks);
        for item in &named.unnamed {
            warn!(
                discogs_release_id = release_id,
                artist_name = %credit.name,
                role = %credit.role,
                item = %item,
                "a Discogs credit names a track the release's tracklist does not hold"
            );
        }
        if named.rows.is_empty() {
            warn!(
                discogs_release_id = release_id,
                artist_name = %credit.name,
                role = %credit.role,
                tracks = %tracks,
                "dropping a Discogs credit none of whose tracks the tracklist holds"
            );
        }
        for path in named.rows {
            let row = row_at(&mut scoped, &path);
            if row.type_ == "heading" {
                continue;
            }
            row.extraartists
                .get_or_insert_with(Vec::new)
                .push(DiscogsRoleArtist { tracks: None, ..credit.clone() });
        }
    }
    scoped
}

fn row_at<'a>(tracklist: &'a mut [DiscogsTrack], path: &[usize]) -> &'a mut DiscogsTrack {
    let (first, rest) = path.split_first().expect("a row path names a row");
    let row = &mut tracklist[*first];
    if rest.is_empty() {
        row
    } else {
        row_at(&mut row.sub_tracks, rest)
    }
}

#[cfg(test)]
#[path = "discogs_track_scope_tests.rs"]
mod tests;
