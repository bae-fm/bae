use super::*;
use crate::import::source_release::UnfetchedReason;
use std::collections::{HashMap, HashSet, VecDeque};

pub(super) type DocumentKey = (PayloadSource, String);

/// The related entities named by one document — the edges a fetch follows. It
/// never enumerates an album's pressings.
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

/// Optional documents have one admission policy: a parser failure discards
/// that document. The required anchor still uses `related_documents`
/// directly.
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

/// Own the graph walk independently of how each document is retrieved.
struct DocumentTraversal {
    payloads: ReleasePayloads,
    depth: ReadDepth,
    /// The release's own album page: its MusicBrainz release group or its
    /// Discogs master, as its own document names it.
    album: Option<DocumentKey>,
    queue: VecDeque<DocumentKey>,
    seen: HashSet<DocumentKey>,
    /// Documents named by one this walk read that its depth does not follow.
    stopped: Vec<DocumentKey>,
}

impl DocumentTraversal {
    fn new(release: &MetadataRef, anchor: String, depth: ReadDepth) -> Result<Self, ImportError> {
        let source = PayloadSource::release_of(release.catalog);
        let edges = related_documents(source, &release.key, &anchor)?;
        let payloads = ReleasePayloads {
            release: release.clone(),
            anchor,
            supporting: Vec::new(),
            unfetched: Vec::new(),
        };
        let album = payloads.anchor_parent()?.map(|album| match album.catalog {
            Catalog::MusicBrainz => (PayloadSource::MusicBrainzReleaseGroup, album.key),
            Catalog::Discogs => (PayloadSource::DiscogsMaster, album.key),
            other => not_fetched(other),
        });
        let mut traversal = Self {
            payloads,
            depth,
            album,
            queue: VecDeque::new(),
            seen: HashSet::from([(source, release.key.clone())]),
            stopped: Vec::new(),
        };
        traversal.admit(None, edges);
        Ok(traversal)
    }

    /// Whether this walk follows the link `from` names to `to`; `from` is
    /// `None` for the release's own page. The one rule a read's depth
    /// decides.
    ///
    /// An identification read follows what [`ReleaseDocument`] and the
    /// release's album statements read: the release's own album page, and
    /// for a MusicBrainz release the Wikidata items and Discogs releases its
    /// own page and its release group's page link. A Discogs release's album
    /// statements are none, so nothing it links on MusicBrainz is followed.
    ///
    /// [`ReleaseDocument`]: crate::identify::documents::ReleaseDocument
    fn follows(&self, from: Option<&DocumentKey>, to: &DocumentKey) -> bool {
        match self.depth {
            ReadDepth::Complete => true,
            ReadDepth::Identification => {
                let musicbrainz = self.payloads.release.catalog == Catalog::MusicBrainz;
                match from {
                    None => {
                        self.album.as_ref() == Some(to)
                            || (musicbrainz
                                && matches!(to.0, PayloadSource::Wikidata | PayloadSource::Discogs))
                    }
                    Some(from) => {
                        musicbrainz
                            && self.album.as_ref() == Some(from)
                            && to.0 == PayloadSource::Wikidata
                    }
                }
            }
        }
    }

    /// Queue what `from` links that this walk follows, and set the rest aside.
    fn admit(&mut self, from: Option<&DocumentKey>, edges: Vec<DocumentKey>) {
        for edge in edges {
            if self.follows(from, &edge) {
                self.queue.push_back(edge);
            } else {
                self.stopped.push(edge);
            }
        }
    }

    fn next(&mut self) -> Option<DocumentKey> {
        while let Some(key) = self.queue.pop_front() {
            if self.seen.insert(key.clone()) {
                return Some(key);
            }
        }
        None
    }

    fn accept(&mut self, document: SourcePayload) {
        if let Some(edges) = supporting_document_edges(
            document.source,
            &document.source_release_id,
            &document.json,
        ) {
            let from = (document.source, document.source_release_id.clone());
            self.admit(Some(&from), edges);
            self.payloads.supporting.push(document);
        }
    }

    fn missing(&mut self, source: PayloadSource, key: String, reason: UnfetchedReason) {
        self.payloads.unfetched.push(UnfetchedDocument {
            document: source,
            key,
            reason,
        });
    }

    /// The documents the walk read, naming each one it set aside and never
    /// reached through another link as one it stopped short of.
    fn finish(mut self) -> ReleasePayloads {
        for (source, key) in std::mem::take(&mut self.stopped) {
            if self.seen.insert((source, key.clone())) {
                self.missing(source, key, UnfetchedReason::StoppedShort);
            }
        }
        self.payloads
    }
}

/// What asking for one document came to.
enum Fetched {
    Document(String),
    /// The catalog answered that there is none to give.
    Absent,
    /// A Discogs document, and no Discogs client to ask with.
    DiscogsNotConfigured,
    /// The same entity was asked for under another key in this walk and its
    /// source failed.
    FailedEarlier,
}

struct FetchDocuments<'a> {
    musicbrainz: &'a crate::musicbrainz::MusicBrainz,
    wikidata: &'a crate::wikidata::Wikidata,
    discogs: Option<&'a DiscogsClient>,
    priority: CallPriority,
    documents: HashMap<DocumentKey, String>,
    attempted_musicbrainz: HashSet<DocumentKey>,
}

impl FetchDocuments<'_> {
    async fn musicbrainz_document(
        &mut self,
        source: PayloadSource,
        id: &str,
    ) -> Result<Fetched, ImportError> {
        let key = (source, id.to_owned());
        if let Some(json) = self.documents.get(&key) {
            return Ok(Fetched::Document(json.clone()));
        }
        if !self.attempted_musicbrainz.insert(key.clone()) {
            return Ok(Fetched::FailedEarlier);
        }
        let json = match source {
            PayloadSource::MusicBrainz => {
                self.musicbrainz
                    .lookup_release_by_id(id, self.priority)
                    .await?
                    .1
            }
            PayloadSource::MusicBrainzReleaseGroup => {
                self.musicbrainz
                    .fetch_release_group_json(id, self.priority)
                    .await?
            }
            _ => unreachable!("MusicBrainz entity fetch requires a release or group"),
        };
        self.documents.insert(key, json.clone());
        Ok(Fetched::Document(json))
    }

    async fn document(
        &mut self,
        source: PayloadSource,
        id: &str,
    ) -> Result<Fetched, ImportError> {
        if let Some(json) = self.documents.get(&(source, id.to_owned())) {
            return Ok(Fetched::Document(json.clone()));
        }
        let json = match source {
            PayloadSource::MusicBrainz | PayloadSource::MusicBrainzReleaseGroup => {
                return self.musicbrainz_document(source, id).await
            }
            PayloadSource::Discogs => {
                let Some(client) = self.discogs else {
                    return Ok(Fetched::DiscogsNotConfigured);
                };
                client.get_release(id, self.priority).await?.1
            }
            PayloadSource::DiscogsMaster => {
                let Some(client) = self.discogs else {
                    return Ok(Fetched::DiscogsNotConfigured);
                };
                client.get_master(id, self.priority).await?.1
            }
            PayloadSource::MusicBrainzDiscogsXref | PayloadSource::MusicBrainzDiscogsMasterXref => {
                let found = match source {
                    PayloadSource::MusicBrainzDiscogsXref => {
                        self.musicbrainz
                            .lookup_releases_by_discogs_release(id, self.priority)
                            .await?
                    }
                    PayloadSource::MusicBrainzDiscogsMasterXref => {
                        self.musicbrainz
                            .lookup_groups_by_discogs_master(id, self.priority)
                            .await?
                    }
                    _ => unreachable!(),
                };
                let Some((pages, _raw)) = found else {
                    return Ok(Fetched::Absent);
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
                    return Ok(Fetched::Absent);
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
                match self.musicbrainz_document(canonical.0, canonical.1).await? {
                    Fetched::Document(json) => json,
                    other => return Ok(other),
                }
            }
            PayloadSource::Wikidata => self
                .wikidata
                .fetch_entity(id, self.priority)
                .await
                .map_err(|error| ImportError::SourceData {
                    catalog: Catalog::Wikidata,
                    detail: error.to_string(),
                })?,
        };
        self.documents.insert((source, id.to_owned()), json.clone());
        Ok(Fetched::Document(json))
    }
}

/// The documents `release` links to, as far as `depth` follows them, asked of
/// MusicBrainz, Wikidata and — when a client is given — Discogs.
pub(crate) async fn fetch_documents(
    musicbrainz: &crate::musicbrainz::MusicBrainz,
    wikidata: &crate::wikidata::Wikidata,
    discogs: Option<&DiscogsClient>,
    release: &MetadataRef,
    depth: ReadDepth,
    priority: CallPriority,
) -> Result<ReleasePayloads, ImportError> {
    let mut fetcher = FetchDocuments {
        musicbrainz,
        wikidata,
        discogs,
        priority,
        documents: HashMap::new(),
        attempted_musicbrainz: HashSet::new(),
    };
    let anchor = match fetcher
        .document(PayloadSource::release_of(release.catalog), &release.key)
        .await?
    {
        Fetched::Document(json) => json,
        Fetched::DiscogsNotConfigured => return Err(ImportError::DiscogsNotConfigured),
        Fetched::Absent | Fetched::FailedEarlier => {
            unreachable!("a release's own document is asked for first and by its own id")
        }
    };
    let mut traversal = DocumentTraversal::new(release, anchor, depth)?;
    while let Some((source, key)) = traversal.next() {
        match fetcher.document(source, &key).await {
            Ok(Fetched::Document(json)) => traversal.accept(SourcePayload::new(source, key, json)),
            Ok(Fetched::Absent) => {
                tracing::debug!(?source, entity = key, "Supporting metadata document does not exist");
            }
            Ok(Fetched::DiscogsNotConfigured) => {
                tracing::debug!(?source, entity = key, "Supporting Discogs document needs a Discogs key");
                traversal.missing(source, key, UnfetchedReason::DiscogsNotConfigured);
            }
            Ok(Fetched::FailedEarlier) => {
                tracing::debug!(?source, entity = key, "Supporting metadata document failed under another key");
                traversal.missing(source, key, UnfetchedReason::Failed);
            }
            Err(error) => {
                warn!(?source, entity = key, %error, "Supporting metadata could not be fetched");
                traversal.missing(source, key, UnfetchedReason::Failed);
            }
        }
    }
    Ok(traversal.finish())
}

#[cfg(test)]
#[path = "traversal_tests.rs"]
mod tests;
