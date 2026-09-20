use super::*;
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) type DocumentKey = (PayloadSource, String);

/// The related entities named by one document. Online fetching and archive
/// replay follow these same edges; neither enumerates an album's pressings.
pub(super) fn related_documents(
    source: PayloadSource,
    key: &str,
    json: &str,
) -> Result<Vec<DocumentKey>, ImportError> {
    let mut next = Vec::new();
    let (urls, album) = match source {
        PayloadSource::MusicBrainz | PayloadSource::MusicBrainzDiscogsXref => {
            let release: MbReleaseResponse = serde_json::from_str(json).map_err(mb_data)?;
            if source == PayloadSource::MusicBrainzDiscogsXref {
                next.push((PayloadSource::MusicBrainz, release.id.clone()));
            }
            if let Some(group) = &release.release_group {
                next.push((PayloadSource::MusicBrainzReleaseGroup, group.id.clone()));
            }
            if let Some(relations) = release
                .release_group
                .as_ref()
                .and_then(|group| group.relations.as_ref())
            {
                for page in
                    crate::musicbrainz::relation_urls(relations).filter_map(parse_catalog_url)
                {
                    if let Some(request) = page_document(&page, true) {
                        next.push(request);
                    }
                }
            }
            (
                crate::musicbrainz::relation_urls(&release.relations)
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
                false,
            )
        }
        PayloadSource::MusicBrainzReleaseGroup | PayloadSource::MusicBrainzDiscogsMasterXref => {
            let group = crate::musicbrainz::parse_release_group(json).map_err(mb_data)?;
            if source == PayloadSource::MusicBrainzDiscogsMasterXref {
                next.push((PayloadSource::MusicBrainzReleaseGroup, group.id.clone()));
            }
            (
                crate::musicbrainz::relation_urls(&group.relations)
                    .map(str::to_owned)
                    .collect(),
                true,
            )
        }
        PayloadSource::Discogs => {
            let release = crate::discogs::client::parse_discogs_release_json(json)?;
            if let Some(master) = release.master_id {
                next.push((PayloadSource::DiscogsMaster, master));
            }
            next.push((PayloadSource::MusicBrainzDiscogsXref, key.to_owned()));
            (Vec::new(), false)
        }
        PayloadSource::DiscogsMaster => {
            crate::discogs::client::parse_discogs_master_json(json)?;
            next.push((PayloadSource::MusicBrainzDiscogsMasterXref, key.to_owned()));
            (Vec::new(), true)
        }
        PayloadSource::Wikidata => {
            let item =
                crate::wikidata::parse_entity(json).map_err(|error| ImportError::SourceData {
                    catalog: Catalog::Wikidata,
                    detail: error.to_string(),
                })?;
            for page in item.catalog_pages() {
                if let Some(request) = page_document(&page, true) {
                    next.push(request);
                }
            }
            (Vec::new(), true)
        }
    };
    for page in urls.iter().filter_map(|url| parse_catalog_url(url)) {
        if let Some(request) = page_document(&page, album) {
            next.push(request);
        }
    }
    let mut seen = HashSet::new();
    next.retain(|key| seen.insert(key.clone()));
    next.sort_by(|left, right| (left.0.as_str(), &left.1).cmp(&(right.0.as_str(), &right.1)));
    Ok(next)
}

/// Optional documents have one admission policy for network lookups, archived
/// reads, and frozen metadata applications. Parser failures discard that
/// document; the required anchor still uses `related_documents` directly.
pub(super) fn supporting_document_edges(
    source: PayloadSource,
    key: &str,
    json: &str,
) -> Option<Vec<DocumentKey>> {
    match related_documents(source, key, json) {
        Ok(edges) => Some(edges),
        Err(error) => {
            warn!(?source, entity = key, %error, "Skipping malformed supporting metadata document");
            None
        }
    }
}

fn mb_data(error: serde_json::Error) -> ImportError {
    ImportError::SourceData {
        catalog: Catalog::MusicBrainz,
        detail: error.to_string(),
    }
}

fn page_document(page: &CatalogPage, album_context: bool) -> Option<DocumentKey> {
    match page {
        CatalogPage::Release {
            catalog: Catalog::Discogs,
            key,
        } if !album_context => Some((PayloadSource::Discogs, key.clone())),
        CatalogPage::Release {
            catalog: Catalog::MusicBrainz,
            key,
        } if !album_context => Some((PayloadSource::MusicBrainz, key.clone())),
        CatalogPage::Group {
            catalog: Catalog::Discogs,
            key,
        } => Some((PayloadSource::DiscogsMaster, key.clone())),
        CatalogPage::Group {
            catalog: Catalog::MusicBrainz,
            key,
        } => Some((PayloadSource::MusicBrainzReleaseGroup, key.clone())),
        CatalogPage::Release {
            catalog: Catalog::Wikidata,
            key,
        }
        | CatalogPage::Group {
            catalog: Catalog::Wikidata,
            key,
        } => Some((PayloadSource::Wikidata, key.clone())),
        _ => None,
    }
}

struct FetchDocuments<'a> {
    discogs: Option<&'a DiscogsClient>,
    priority: CallPriority,
    documents: HashMap<DocumentKey, String>,
    attempted_musicbrainz: HashSet<DocumentKey>,
}

impl FetchDocuments<'_> {
    async fn musicbrainz(
        &mut self,
        source: PayloadSource,
        id: &str,
    ) -> Result<Option<String>, ImportError> {
        let key = (source, id.to_owned());
        if let Some(json) = self.documents.get(&key) {
            return Ok(Some(json.clone()));
        }
        if !self.attempted_musicbrainz.insert(key.clone()) {
            return Ok(None);
        }
        let json = match source {
            PayloadSource::MusicBrainz => {
                crate::musicbrainz::lookup_release_by_id(id, self.priority)
                    .await?
                    .1
            }
            PayloadSource::MusicBrainzReleaseGroup => {
                crate::musicbrainz::fetch_release_group_json(id, self.priority).await?
            }
            _ => unreachable!("MusicBrainz entity fetch requires a release or group"),
        };
        self.documents.insert(key, json.clone());
        Ok(Some(json))
    }

    async fn document(
        &mut self,
        source: PayloadSource,
        id: &str,
    ) -> Result<Option<String>, ImportError> {
        if let Some(json) = self.documents.get(&(source, id.to_owned())) {
            return Ok(Some(json.clone()));
        }
        let json = match source {
            PayloadSource::MusicBrainz | PayloadSource::MusicBrainzReleaseGroup => {
                return self.musicbrainz(source, id).await
            }
            PayloadSource::Discogs => {
                let Some(client) = self.discogs else {
                    return Ok(None);
                };
                client.get_release(id, self.priority).await?.1
            }
            PayloadSource::DiscogsMaster => {
                let Some(client) = self.discogs else {
                    return Ok(None);
                };
                client.get_master(id, self.priority).await?.1
            }
            PayloadSource::MusicBrainzDiscogsXref | PayloadSource::MusicBrainzDiscogsMasterXref => {
                let found = match source {
                    PayloadSource::MusicBrainzDiscogsXref => {
                        crate::musicbrainz::lookup_releases_by_discogs_release(id, self.priority)
                            .await?
                    }
                    PayloadSource::MusicBrainzDiscogsMasterXref => {
                        crate::musicbrainz::lookup_groups_by_discogs_master(id, self.priority)
                            .await?
                    }
                    _ => unreachable!(),
                };
                let Some((pages, _raw)) = found else {
                    return Ok(None);
                };
                let mut unique = Vec::new();
                for page in pages {
                    if !unique.contains(&page) {
                        unique.push(page);
                    }
                }
                let [page] = unique.as_slice() else {
                    if !unique.is_empty() {
                        warn!(
                            discogs_id = id,
                            ?source,
                            count = unique.len(),
                            "MusicBrainz URL has ambiguous counterparts; no identity claimed"
                        );
                    }
                    return Ok(None);
                };
                let canonical = match page {
                    CatalogPage::Release {
                        catalog: Catalog::MusicBrainz,
                        key,
                    } => (PayloadSource::MusicBrainz, key),
                    CatalogPage::Group {
                        catalog: Catalog::MusicBrainz,
                        key,
                    } => (PayloadSource::MusicBrainzReleaseGroup, key),
                    _ => unreachable!(
                        "URL lookup returns only MusicBrainz targets of the requested kind"
                    ),
                };
                let Some(json) = self.musicbrainz(canonical.0, canonical.1).await? else {
                    return Ok(None);
                };
                json
            }
            PayloadSource::Wikidata => crate::wikidata::fetch_entity(id, self.priority)
                .await
                .map_err(|error| ImportError::SourceData {
                    catalog: Catalog::Wikidata,
                    detail: error.to_string(),
                })?,
        };
        self.documents.insert((source, id.to_owned()), json.clone());
        Ok(Some(json))
    }
}

pub(super) async fn fetch_documents(
    discogs: Option<&DiscogsClient>,
    release: &MetadataRef,
    stored: Option<&ReleasePayloads>,
    priority: CallPriority,
) -> Result<ReleasePayloads, ImportError> {
    let mut fetcher = FetchDocuments {
        discogs,
        priority,
        documents: HashMap::new(),
        attempted_musicbrainz: HashSet::new(),
    };
    if let Some(stored) = stored {
        fetcher.documents.insert(
            (
                PayloadSource::release_of(release.catalog),
                release.key.clone(),
            ),
            stored.anchor.clone(),
        );
        fetcher
            .documents
            .extend(stored.supporting.iter().map(|document| {
                (
                    (document.source, document.source_release_id.clone()),
                    document.json.clone(),
                )
            }));
        for document in stored.supporting.iter() {
            let canonical = match document.source {
                PayloadSource::MusicBrainzDiscogsXref => {
                    let release: MbReleaseResponse =
                        serde_json::from_str(&document.json).map_err(mb_data)?;
                    Some((PayloadSource::MusicBrainz, release.id))
                }
                PayloadSource::MusicBrainzDiscogsMasterXref => {
                    let group =
                        crate::musicbrainz::parse_release_group(&document.json).map_err(mb_data)?;
                    Some((PayloadSource::MusicBrainzReleaseGroup, group.id))
                }
                _ => None,
            };
            if let Some(canonical) = canonical {
                fetcher
                    .documents
                    .entry(canonical)
                    .or_insert_with(|| document.json.clone());
            }
        }
    }
    let anchor_key = (
        PayloadSource::release_of(release.catalog),
        release.key.clone(),
    );
    if release.catalog == Catalog::Discogs && discogs.is_none() && stored.is_none() {
        return Err(ImportError::DiscogsNotConfigured);
    }
    let anchor = fetcher
        .document(anchor_key.0, &anchor_key.1)
        .await?
        .ok_or(ImportError::DiscogsNotConfigured)?;
    let mut supporting = Vec::new();
    let mut seen = HashSet::from([anchor_key.clone()]);
    let mut queue: VecDeque<_> = related_documents(anchor_key.0, &anchor_key.1, &anchor)?.into();
    while let Some((source, key)) = queue.pop_front() {
        if !seen.insert((source, key.clone())) {
            continue;
        }
        let json = match fetcher.document(source, &key).await {
            Ok(Some(json)) => json,
            Ok(None) => continue,
            Err(error) => {
                warn!(?source, entity = key, %error, "Supporting metadata could not be fetched");
                continue;
            }
        };
        if let Some(edges) = supporting_document_edges(source, &key, &json) {
            queue.extend(edges);
            supporting.push(SourcePayload::new(source, key, json));
        }
    }
    Ok(ReleasePayloads {
        release: release.clone(),
        anchor,
        supporting,
    })
}

pub(super) fn load_documents(
    documents: &impl ArchivedDocuments,
    release: &MetadataRef,
) -> Result<Option<ReleasePayloads>, ImportError> {
    let source = PayloadSource::release_of(release.catalog);
    let Some(anchor) = documents.document(source, &release.key)? else {
        return Ok(None);
    };
    let mut seen = HashSet::from([(source, release.key.clone())]);
    let mut queue: VecDeque<_> = related_documents(source, &release.key, &anchor)?.into();
    let mut supporting = Vec::new();
    while let Some((source, key)) = queue.pop_front() {
        if !seen.insert((source, key.clone())) {
            continue;
        }
        if let Some(json) = documents.document(source, &key)? {
            if let Some(edges) = supporting_document_edges(source, &key, &json) {
                queue.extend(edges);
                supporting.push(SourcePayload::new(source, key, json));
            }
        }
    }
    Ok(Some(ReleasePayloads {
        release: release.clone(),
        anchor,
        supporting,
    }))
}

#[cfg(test)]
#[path = "traversal_tests.rs"]
mod tests;
