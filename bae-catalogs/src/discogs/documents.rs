//! How Discogs's release and master documents read into bae's Discogs
//! models: the JSON as Discogs serves it, and the projections every fetch and
//! every archived document go through.

use super::client::DiscogsError;
use crate::discogs::models::{
    DiscogsArtist, DiscogsMaster, DiscogsRelease, DiscogsRoleArtist, DiscogsTrack,
};
use crate::discogs::remote_cover_from_urls;
use crate::remote_image::{CoverLabel, RemoteCover};
use serde::Deserialize;
use tracing::warn;

#[derive(Debug, Deserialize, Clone)]
struct ArtistCredit {
    id: u64,
    #[serde(deserialize_with = "name_without_suffix")]
    name: String,
    // Discogs prints "" for neither; the mapper reads an empty one as none.
    #[serde(default)]
    anv: Option<String>,
    #[serde(default)]
    join: Option<String>,
}

impl ArtistCredit {
    fn into_model(self) -> DiscogsArtist {
        DiscogsArtist { id: self.id.to_string(), name: self.name, anv: self.anv, join: self.join }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct ExtraArtistCredit {
    id: Option<u64>,
    #[serde(deserialize_with = "name_without_suffix")]
    name: String,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    role: Option<String>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    anv: Option<String>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    tracks: Option<String>,
}
#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    id: u64,
    title: String,
    year: Option<u32>,
    formats: Option<Vec<crate::discogs::DiscogsFormat>>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    country: Option<String>,
    labels: Option<Vec<LabelResponse>>,
    images: Option<Vec<Image>>,
    artists: Option<Vec<ArtistCredit>>,
    extraartists: Option<Vec<ExtraArtistCredit>>,
    tracklist: Option<Vec<TrackResponse>>,
    #[serde(default, deserialize_with = "optional_master_id")]
    master_id: Option<u64>,
    identifiers: Option<Vec<Identifier>>,
    companies: Option<Vec<Company>>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    notes: Option<String>,
}

/// A company the release credits, whatever its role.
#[derive(Debug, Deserialize)]
struct Company {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Identifier {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    value: Option<String>,
}

/// Discogs uses zero when a release has no master record.
pub(crate) fn optional_master_id<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<u64>::deserialize(deserializer)?.filter(|id| *id != 0))
}

#[derive(Debug, Deserialize)]
struct MasterResponse {
    id: u64,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    title: Option<String>,
    year: Option<u32>,
    artists: Option<Vec<ArtistCredit>>,
    images: Option<Vec<Image>>,
}
#[derive(Debug, Deserialize)]
struct Image {
    #[serde(rename = "type")]
    image_type: String,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    uri: Option<String>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    uri150: Option<String>,
}

/// Discogs uses the same image array on releases and masters. Keep every
/// usable image, with the primary image first and provider order otherwise.
fn image_covers(images: Option<Vec<Image>>, entity: &str, id: u64) -> Vec<RemoteCover> {
    let mut images: Vec<Image> = images.into_iter().flatten().collect();
    images.sort_by_key(|image| image.image_type != "primary");
    let mut covers = Vec::new();
    for (index, image) in images.into_iter().enumerate() {
        if let Some(mut cover) =
            remote_cover_from_urls(image.uri.as_deref(), image.uri150.as_deref(), entity, id)
        {
            cover.label = CoverLabel::Gallery {
                reference: Some(format!("{entity}{id}")),
                position: index as u32 + 1,
                types: Vec::new(),
                comment: String::new(),
            };
            crate::remote_image::push_unique_cover(&mut covers, cover);
        }
    }
    covers
}

/// Artwork from an archived master document, using the release image parser.
pub fn parse_discogs_master_covers(raw_json: &str) -> Result<Vec<RemoteCover>, DiscogsError> {
    Ok(parse_discogs_master_json(raw_json)?.covers)
}
#[derive(Debug, Deserialize)]
struct TrackResponse {
    position: String,
    title: String,
    duration: Option<String>,
    #[serde(default)]
    artists: Vec<ArtistCredit>,
    extraartists: Option<Vec<ExtraArtistCredit>>,
    #[serde(default)]
    type_: String,
    #[serde(default)]
    sub_tracks: Vec<TrackResponse>,
}
#[derive(Debug, Deserialize)]
struct LabelResponse {
    /// Discogs' id for the label.
    #[serde(default)]
    id: Option<u64>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    name: Option<String>,
    #[serde(default, deserialize_with = "crate::serde_helpers::empty_string_as_none")]
    catno: Option<String>,
}

/// A Discogs name without the " (n)" Discogs appends to tell apart two
/// entities of one name ("Label Name Records (2)", "Artist Name (3)"): the name the entity goes by.
pub(crate) fn without_suffix(name: &str) -> String {
    let trimmed = name.trim_end();
    if let Some(open) = trimmed.strip_suffix(')').and_then(|rest| rest.rfind(" (")) {
        let number = &trimmed[open + 2..trimmed.len() - 1];
        if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
            return trimmed[..open].to_string();
        }
    }
    trimmed.to_string()
}

/// Discogs' "Not On Label" label, and the id it holds.
const NOT_ON_LABEL_ID: u64 = 1818;

/// Whether a Discogs label says its release has no label: "Not On Label", or
/// its per-artist form "Not On Label (Name Self-released)".
pub(crate) fn is_not_on_label(name: &str) -> bool {
    let name = name.trim();
    name.strip_prefix("Not On Label").is_some_and(|rest| rest.is_empty() || rest.starts_with(" ("))
}

/// One Discogs name, [`without_suffix`].
fn name_without_suffix<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(without_suffix(&String::deserialize(deserializer)?))
}

/// A list of Discogs names, each [`without_suffix`].
pub(crate) fn names_without_suffix<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let names: Option<Vec<String>> = Option::deserialize(deserializer)?;
    Ok(names.map(|names| names.iter().map(|name| without_suffix(name)).collect()))
}

fn extra_artist_to_model(a: ExtraArtistCredit) -> Option<DiscogsRoleArtist> {
    let Some(role) = a.role else {
        warn!(
            discogs_artist_id = ?a.id,
            artist_name = %a.name,
            "Skipping Discogs extraartist without role"
        );
        return None;
    };
    Some(DiscogsRoleArtist {
        id: a.id.map(|id| id.to_string()),
        name: a.name,
        role,
        credited_name: a.anv,
        tracks: a.tracks,
    })
}

fn track_to_model(track: TrackResponse) -> DiscogsTrack {
    DiscogsTrack {
        position: track.position,
        title: track.title,
        duration: track.duration,
        artists: track.artists.into_iter().map(ArtistCredit::into_model).collect(),
        extraartists: track.extraartists.map(|extraartists| {
            extraartists.into_iter().filter_map(extra_artist_to_model).collect()
        }),
        type_: if track.type_.is_empty() { "track".to_string() } else { track.type_ },
        sub_tracks: track.sub_tracks.into_iter().map(track_to_model).collect(),
    }
}

/// Raw Discogs release JSON to the public `DiscogsRelease`. The same projection
/// `get_release` applies to a fresh response, exposed as a free function so a
/// release fetch's documents are read the way the client reads them.
pub fn parse_discogs_release_json(raw_json: &str) -> Result<DiscogsRelease, DiscogsError> {
    let release: ReleaseResponse = serde_json::from_str(raw_json)?;
    let tracklist = release.tracklist.unwrap_or_default().into_iter().map(track_to_model).collect();
    let artists =
        release.artists.unwrap_or_default().into_iter().map(ArtistCredit::into_model).collect();
    let extraartists = release
        .extraartists
        .map(|extraartists| extraartists.into_iter().filter_map(extra_artist_to_model).collect());
    let covers = image_covers(release.images, "r", release.id);
    let master_id = release.master_id.map(|id| id.to_string());
    let labels = bae_types::pressing::ReleaseLabel::each_once(
        release.labels.into_iter().flatten().filter_map(|label| {
            let no_label = label.id == Some(NOT_ON_LABEL_ID)
                || label.name.as_deref().is_some_and(is_not_on_label);
            let name = label.name.as_deref().map(without_suffix).filter(|_| !no_label);
            bae_types::pressing::ReleaseLabel::new(name, label.catno)
                .map(|stated| stated.with_ids(None, label.id.map(|id| id.to_string())))
        }),
    );
    let formats = release.formats.unwrap_or_default();
    let (barcodes, identifiers): (Vec<Identifier>, Vec<Identifier>) = release
        .identifiers
        .into_iter()
        .flatten()
        .partition(|identifier| identifier.kind == "Barcode");
    // Discogs often lists one code twice: as printed and as scanned.
    let barcode_values = bae_types::barcode::each_product_once(
        barcodes.into_iter().filter_map(|identifier| identifier.value),
    );
    let identifiers = identifiers.into_iter().filter_map(|identifier| identifier.value).collect();
    let companies = release.companies.into_iter().flatten().map(|company| company.name).collect();

    Ok(DiscogsRelease {
        id: release.id.to_string(),
        title: release.title,
        // Discogs encodes an unknown year as zero.
        year: release.year.filter(|year| *year != 0),
        formats,
        country: release.country,
        labels,
        barcodes: barcode_values,
        covers,
        artists,
        extraartists,
        tracklist,
        master_id,
        companies,
        identifiers,
        notes: release.notes.map(|notes| super::markup::shown_text(&notes)),
    })
}

/// Parse the master's own album metadata for both fresh fetches and archive replay.
pub fn parse_discogs_master_json(raw_json: &str) -> Result<DiscogsMaster, DiscogsError> {
    let master: MasterResponse = serde_json::from_str(raw_json)?;
    Ok(DiscogsMaster {
        title: master.title,
        year: master.year.filter(|year| *year != 0),
        artists: master.artists.into_iter().flatten().map(ArtistCredit::into_model).collect(),
        covers: image_covers(master.images, "m", master.id),
    })
}
