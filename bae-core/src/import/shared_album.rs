//! Linking a folder to the album its offered pressings are of, when the person
//! cannot tell which of them their copy is.
//!
//! A list offers several pressings of one album when its offered rows — the
//! ones above the set-aside disclosure — are at least two, all on one album's
//! card, and every lookup catalog that files them files them under one album:
//! one MusicBrainz release group, one Discogs master. The link names those
//! albums and no pressing; the copy may be one no catalog lists.
//!
//! What the rows agree on is applied to the draft, one field at a time, as the
//! top-ranked row spells it; a field they disagree on, or that one of them
//! leaves unstated, keeps the draft's value. Codes compare exactly — a catalog
//! number by `text_match::catalog_key`, a barcode by
//! `barcode::comparison_key` — album titles by `text_match::bare_album_title`
//! squashed, artists by `text_match::same_artist_name`, label names by
//! `text_match::same_label_name`, and track titles position by position by
//! `text_match::track_title_key`. The tracks agree only where every row lists
//! as many as the folder has.

use crate::import::release_group::ReleaseGroup;
use crate::import::{
    AlbumLink, ArtistAssignment, CandidateDraft, Catalog, MetadataRef, RawLabelEdit,
    RawReleaseEdit, TrackArtistAssignments,
};
use crate::text_match;

/// The offered rows of a list that are several pressings of one album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedAlbum {
    /// Each offered row's lead — the release picking the row reads — in the
    /// list's order, most agreed-with first.
    pub leads: Vec<MetadataRef>,
    /// The album the rows are pressings of.
    pub link: AlbumLink,
}

impl SharedAlbum {
    /// The album a list's cards offer several pressings of; `None` where it
    /// offers fewer than two rows, rows of two cards, or rows a catalog files
    /// under two albums, and where no catalog names their album.
    pub fn of(groups: &[ReleaseGroup]) -> Option<Self> {
        let mut offering = groups
            .iter()
            .filter(|group| group.pressings().next().is_some());
        let card = offering.next()?;
        if offering.next().is_some() {
            return None;
        }
        let rows: Vec<_> = card.pressings().collect();
        if rows.len() < 2 {
            return None;
        }
        let mut albums = Vec::new();
        for catalog in Catalog::LOOKUP {
            let mut named = rows
                .iter()
                .flat_map(|row| &row.releases)
                .filter(|release| release.source == catalog)
                .filter_map(|release| release.source_group_id.as_deref());
            let Some(album) = named.next() else {
                continue;
            };
            if named.any(|other| other != album) {
                return None;
            }
            albums.push(MetadataRef::new(catalog, album));
        }
        Some(Self {
            leads: rows
                .iter()
                .map(|row| {
                    let lead = row.lead();
                    MetadataRef::new(lead.source, lead.release_id.clone())
                })
                .collect(),
            link: AlbumLink::new(albums)?,
        })
    }
}

/// `current` with every field the rows' drafts `offered` agree on — the
/// top-ranked row's first — taken from them, and every other field as it is.
/// Where they share no pressing year, the year is `folder_year`, the year the
/// folder names the pressing by, when it names one.
pub(crate) fn shared_draft(
    current: &CandidateDraft,
    offered: &[RawReleaseEdit],
    folder_year: Option<i32>,
) -> CandidateDraft {
    let mut draft = current.clone();
    if let Some(title) = agreed(
        offered,
        |row| stated(&row.album_title),
        |a, b| same_album_title(a, b),
    ) {
        draft.album_title = title.to_string();
    }
    if let Some(artists) = agreed(
        offered,
        |row| (!row.album_artist_assignments.is_empty()).then_some(&row.album_artist_assignments),
        |a, b| same_artists(a, b),
    ) {
        draft.album_artist_assignments = artists.clone();
    }
    if let Some(year) = agreed(offered, |row| stated(&row.album_year), |a, b| a == b) {
        draft.album_year = year.to_string();
    }
    match agreed(offered, |row| stated(&row.pressing.year), |a, b| a == b) {
        Some(year) => draft.pressing.year = year.to_string(),
        None => {
            if let Some(year) = folder_year {
                draft.pressing.year = year.to_string();
            }
        }
    }
    draft.pressing.labels = shared_labels(&current.pressing.labels, offered);
    if let Some(barcode) = agreed(
        offered,
        |row| {
            crate::barcode::comparison_key(&row.pressing.barcode)
                .ok()
                .map(|key| (key, row.pressing.barcode.trim()))
        },
        |a, b| a.0 == b.0,
    ) {
        draft.pressing.barcode = barcode.1.to_string();
    }
    let facts = &mut draft.pressing.facts;
    if let Some(area) = agreed(offered, |row| row.pressing.facts.area, |a, b| a == b) {
        facts.area = Some(area);
    }
    if let Some(media) = agreed(
        offered,
        |row| (!row.pressing.facts.media.is_empty()).then_some(&row.pressing.facts.media),
        |a, b| a == b,
    ) {
        facts.media = media.clone();
    }
    if let Some(status) = agreed(offered, |row| row.pressing.facts.status, |a, b| a == b) {
        facts.status = Some(status);
    }
    if let Some(packaging) = agreed(offered, |row| row.pressing.facts.packaging, |a, b| a == b) {
        facts.packaging = Some(packaging);
    }
    if let Some(details) = agreed(
        offered,
        |row| {
            (!row.pressing.facts.discogs_details.is_empty())
                .then_some(&row.pressing.facts.discogs_details)
        },
        |a, b| a == b,
    ) {
        facts.discogs_details = details.clone();
    }
    let tracks_agree = offered
        .iter()
        .all(|row| row.tracks.len() == current.tracks.len());
    if tracks_agree {
        for (position, track) in draft.tracks.iter_mut().enumerate() {
            let edit = &mut track.edit;
            if let Some(title) = agreed(
                offered,
                |row| stated(&row.tracks[position].title),
                |a, b| same_track_title(a, b),
            ) {
                edit.title = title.to_string();
            }
            if let Some(artists) = agreed(
                offered,
                |row| Some(&row.tracks[position].artist_assignments),
                |a, b| same_track_artists(a, b),
            ) {
                edit.artist_assignments = artists.clone();
            }
            if let Some(side) = agreed(offered, |row| row.tracks[position].side, |a, b| a == b) {
                edit.side = Some(side);
            }
            if let Some(number) = agreed(
                offered,
                |row| row.tracks[position].track_number,
                |a, b| a == b,
            ) {
                edit.track_number = number;
            }
        }
    }
    draft
}

/// The value every row states of one field, as the first row states it, when
/// they all agree on it; `None` when a row leaves it unstated or two rows
/// disagree.
fn agreed<'a, T>(
    rows: &'a [RawReleaseEdit],
    value: impl Fn(&'a RawReleaseEdit) -> Option<T>,
    same: impl Fn(&T, &T) -> bool,
) -> Option<T> {
    agreed_values(rows.iter().map(value), same)
}

/// The text of a field that states something, trimmed.
fn stated(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty()).then_some(text)
}

fn same_album_title(a: &str, b: &str) -> bool {
    let key = |title: &str| text_match::squash(&text_match::bare_album_title(title));
    let a = key(a);
    !a.is_empty() && a == key(b)
}

fn same_track_title(a: &str, b: &str) -> bool {
    let a = text_match::track_title_key(a);
    !a.is_empty() && a == text_match::track_title_key(b)
}

fn same_artists(a: &[ArtistAssignment], b: &[ArtistAssignment]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| text_match::same_artist_name(a.name(), b.name()))
}

fn same_track_artists(a: &TrackArtistAssignments, b: &TrackArtistAssignments) -> bool {
    match (a, b) {
        (TrackArtistAssignments::AlbumArtists, TrackArtistAssignments::AlbumArtists) => true,
        (TrackArtistAssignments::Explicit(a), TrackArtistAssignments::Explicit(b)) => {
            same_artists(a, b)
        }
        (TrackArtistAssignments::AlbumArtists, TrackArtistAssignments::Explicit(_))
        | (TrackArtistAssignments::Explicit(_), TrackArtistAssignments::AlbumArtists) => false,
    }
}

/// The draft's label rows with each name and catalog number the rows agree
/// on in its place. Rows that list different numbers of labels agree on none
/// of them, so the draft's stand.
fn shared_labels(current: &[RawLabelEdit], offered: &[RawReleaseEdit]) -> Vec<RawLabelEdit> {
    let listed = |row: &RawReleaseEdit| {
        row.pressing
            .labels
            .iter()
            .filter(|label| !label.is_blank())
            .cloned()
            .collect::<Vec<_>>()
    };
    let lists: Vec<Vec<RawLabelEdit>> = offered.iter().map(listed).collect();
    let Some(count) = lists.first().map(Vec::len) else {
        return current.to_vec();
    };
    if lists.iter().any(|list| list.len() != count) {
        return current.to_vec();
    }
    let mut labels = current.to_vec();
    if labels.len() < count {
        labels.resize(count, RawLabelEdit::default());
    }
    for (position, label) in labels.iter_mut().enumerate().take(count) {
        let names = lists.iter().map(|list| stated(&list[position].name));
        if let Some(name) = agreed_values(names, |a, b| text_match::same_label_name(a, b)) {
            label.name = name.to_string();
        }
        let numbers = lists.iter().map(|list| {
            let number = &list[position].catalog_number;
            text_match::catalog_key(number).map(|key| (key, number.trim()))
        });
        if let Some((_, number)) = agreed_values(numbers, |a, b| a.0 == b.0) {
            label.catalog_number = number.to_string();
        }
    }
    labels
}

/// The first of `values` when every one is stated and agrees with it.
fn agreed_values<T>(
    mut values: impl Iterator<Item = Option<T>>,
    same: impl Fn(&T, &T) -> bool,
) -> Option<T> {
    let first = values.next()??;
    for other in values {
        if !same(&first, &other?) {
            return None;
        }
    }
    Some(first)
}

#[cfg(test)]
#[path = "shared_album_tests.rs"]
mod tests;
