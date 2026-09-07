//! Provider-document relationships used by storage and schema upgrades.
//!
//! This decoder has no provider client or import runtime dependency: libraries
//! migrate on mobile too. It reads relationship fields, not editor metadata.

use serde::Deserialize;

pub(crate) struct ProviderDocument {
    pub(crate) release: Option<DocumentRelease>,
    pub(crate) references: Vec<(&'static str, String)>,
}

pub(crate) struct DocumentRelease {
    pub(crate) id: String,
    pub(crate) group_id: Option<String>,
}

#[derive(Deserialize)]
struct MusicBrainzLinks {
    id: String,
    #[serde(rename = "release-group")]
    release_group: Option<MusicBrainzGroupLinks>,
    #[serde(default)]
    relations: Vec<UrlRelation>,
}

#[derive(Deserialize)]
struct MusicBrainzGroupLinks {
    id: String,
    relations: Option<Vec<UrlRelation>>,
}

#[derive(Deserialize)]
struct UrlRelation {
    url: Option<UrlResource>,
}

#[derive(Deserialize)]
struct UrlResource {
    resource: Option<String>,
}

#[derive(Deserialize)]
struct DiscogsLinks {
    id: u64,
    master_id: Option<u64>,
}

/// Decode the facts stored atomically with a provider document. Targets can be
/// absent: a later fetch under the referenced key becomes visible to readers.
pub(crate) fn decode(source: &str, id: &str, json: &str) -> Result<ProviderDocument, String> {
    let invalid =
        |error: serde_json::Error| format!("stored {source} document {id} does not parse: {error}");
    match source {
        "musicbrainz" => {
            let document: MusicBrainzLinks = serde_json::from_str(json).map_err(invalid)?;
            let source_group_id = document
                .release_group
                .as_ref()
                .map(|group| group.id.clone());
            let mut references = Vec::new();
            if let Some(group_id) = &source_group_id {
                references.push(("musicbrainz_release_group", group_id.clone()));
            }
            let url = first_discogs_url(&document.relations).or_else(|| {
                document
                    .release_group
                    .as_ref()
                    .and_then(|group| group.relations.as_deref())
                    .and_then(first_discogs_url)
            });
            if let Some(release_id) = url.and_then(extract_discogs_release_id) {
                references.push(("discogs", release_id));
            }
            Ok(ProviderDocument {
                release: Some(DocumentRelease {
                    id: document.id.to_string(),
                    group_id: source_group_id,
                }),
                references,
            })
        }
        "discogs" => {
            let document: DiscogsLinks = serde_json::from_str(json).map_err(invalid)?;
            let source_group_id = document.master_id.map(|id| id.to_string());
            let mut references = vec![("musicbrainz_discogs_xref", id.to_string())];
            if let Some(master_id) = &source_group_id {
                references.push(("discogs_master", master_id.clone()));
            }
            Ok(ProviderDocument {
                release: Some(DocumentRelease {
                    id: document.id.to_string(),
                    group_id: source_group_id,
                }),
                references,
            })
        }
        "musicbrainz_release_group" | "musicbrainz_discogs_xref" | "discogs_master" => {
            // Supporting documents do not expand the release's traversal.
            serde_json::from_str::<serde::de::IgnoredAny>(json).map_err(invalid)?;
            Ok(ProviderDocument {
                release: None,
                references: Vec::new(),
            })
        }
        _ => Err(format!("unknown payload source: {source}")),
    }
}

fn first_discogs_url(relations: &[UrlRelation]) -> Option<&str> {
    first_discogs_release_url(
        relations
            .iter()
            .filter_map(|relation| relation.url.as_ref()?.resource.as_deref()),
    )
}

/// Select the same release link during provider lookup and document storage.
pub(crate) fn first_discogs_release_url<'a>(
    urls: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    urls.into_iter()
        .find(|resource| resource.contains("discogs.com/release/"))
}

/// Discogs URLs can end in a numeric ID, a trailing slash, or an ID-title slug.
pub(crate) fn extract_discogs_release_id(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let last = trimmed.rsplit('/').next()?;
    let id: String = last.chars().take_while(|c| c.is_ascii_digit()).collect();
    (!id.is_empty()).then_some(id)
}
