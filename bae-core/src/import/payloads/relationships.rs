use super::*;
use crate::import::album_links::{names_other_album, AlbumLinks, AlbumStatement, Found};
use std::collections::{HashMap, HashSet};

/// The strongest album claims encountered while following this release's links.
/// An ambiguous claim occupies its catalog so weaker links cannot choose for it.
struct AlbumClaims(HashMap<Catalog, AlbumIdentity>);

enum AlbumIdentity {
    Known(String),
    Ambiguous,
}

impl AlbumClaims {
    fn admit(&mut self, claims: impl IntoIterator<Item = MetadataRef>) {
        let mut candidates: HashMap<Catalog, HashSet<String>> = HashMap::new();
        for claim in claims {
            if !self.0.contains_key(&claim.catalog) {
                candidates
                    .entry(claim.catalog)
                    .or_default()
                    .insert(claim.key);
            }
        }
        for (catalog, keys) in candidates {
            let identity = if keys.len() == 1 {
                AlbumIdentity::Known(keys.into_iter().next().expect("one candidate"))
            } else {
                warn!(
                    ?catalog,
                    ?keys,
                    "Conflicting album identities; no catalog identity claimed"
                );
                AlbumIdentity::Ambiguous
            };
            self.0.insert(catalog, identity);
        }
    }

    fn known(&self) -> Vec<MetadataRef> {
        Catalog::ALL
            .into_iter()
            .filter_map(|catalog| match self.0.get(&catalog) {
                Some(AlbumIdentity::Known(key)) => Some(MetadataRef::new(catalog, key)),
                _ => None,
            })
            .collect()
    }
}

fn album_urls(relations: &[crate::musicbrainz::MbRelation]) -> Vec<MetadataRef> {
    crate::musicbrainz::relation_urls(relations)
        .filter_map(parse_catalog_url)
        .filter_map(|page| match page {
            CatalogPage::Group { catalog, key } => Some(MetadataRef::new(catalog, key)),
            CatalogPage::Release { .. } => None,
        })
        .collect()
}

impl ReleasePayloads {
    pub(super) fn discogs_counterpart_keys(&self) -> Result<Vec<String>, ImportError> {
        let response = self.musicbrainz_anchor()?;
        let mut keys: Vec<_> = crate::musicbrainz::relation_urls(&response.relations)
            .filter_map(parse_catalog_url)
            .filter_map(|page| match page {
                CatalogPage::Release {
                    catalog: Catalog::Discogs,
                    key,
                } => Some(key),
                _ => None,
            })
            .collect();
        keys.sort();
        keys.dedup();
        Ok(keys)
    }

    /// The MusicBrainz release group cross-linked to a Discogs-seeded release,
    /// through the MusicBrainz release linked to it.
    fn counterpart_group(&self) -> Result<Option<MetadataRef>, ImportError> {
        Ok(self.musicbrainz_xref()?.and_then(|release| {
            release
                .release_group
                .map(|group| MetadataRef::new(Catalog::MusicBrainz, group.id))
        }))
    }

    /// What this MusicBrainz release's documents state its album is on the
    /// other lookup catalog: the one rule set a stored release's records and
    /// an identify run's rows both read (see [`crate::import::album_links`]).
    /// Three statements, strongest first; the first that names an album is
    /// the one taken:
    ///
    /// 1. The release group's page — the group's document, and the group
    ///    relations the release embeds — or the release's own page links the
    ///    album.
    /// 2. A Wikidata item that page links states the album.
    /// 3. The release links a Discogs release as itself, and that release's
    ///    document files it under the album. Where it links several, they name
    ///    an album only when every one was read and files it under one, or
    ///    when those read already disagree: one that could not be read, or is
    ///    filed under none, may be of another album.
    ///
    /// `Unread` when nothing named an album and a document one of them reads
    /// was not fetched. Only called down the MusicBrainz arm of a
    /// `self.release.catalog` match.
    pub(crate) fn album_statements(&self) -> Result<AlbumLinks, ImportError> {
        let anchor = self.musicbrainz_anchor()?;
        let mut found = Found::default();

        let mut pages: Vec<CatalogPage> = crate::musicbrainz::relation_urls(&anchor.relations)
            .filter_map(parse_catalog_url)
            .collect();
        if let Some(group) = &anchor.release_group {
            if let Some(relations) = &group.relations {
                pages.extend(crate::musicbrainz::relation_urls(relations).filter_map(parse_catalog_url));
            }
            match self.document(PayloadSource::MusicBrainzReleaseGroup, &group.id) {
                Some(json) => {
                    let document = crate::musicbrainz::parse_release_group(json)
                        .map_err(|error| self.source_data(error.to_string()))?;
                    pages.extend(
                        crate::musicbrainz::relation_urls(&document.relations)
                            .filter_map(parse_catalog_url),
                    );
                }
                None if group.relations.is_none() => {
                    found.unread |= self.was_unfetched(PayloadSource::MusicBrainzReleaseGroup, &group.id);
                }
                None => {}
            }
        }
        for page in &pages {
            if let CatalogPage::Group { catalog, key } = page {
                if names_other_album(*catalog) {
                    found.push(MetadataRef::new(*catalog, key.clone()), AlbumStatement::Page);
                }
            }
        }
        if !found.links.is_empty() {
            return Ok(found.settle());
        }

        let mut items: Vec<&str> = Vec::new();
        for page in &pages {
            if let CatalogPage::Group {
                catalog: Catalog::Wikidata,
                key,
            } = page
            {
                if !items.contains(&key.as_str()) {
                    items.push(key);
                }
            }
        }
        for item in items {
            let Some(json) = self.document(PayloadSource::Wikidata, item) else {
                found.unread |= self.was_unfetched(PayloadSource::Wikidata, item);
                continue;
            };
            let entity = crate::wikidata::parse_entity(json)
                .map_err(|error| self.source_data(error.to_string()))?;
            for page in entity.catalog_pages() {
                if let CatalogPage::Group { catalog, key } = page {
                    if names_other_album(catalog) {
                        found.push(
                            MetadataRef::new(catalog, key),
                            AlbumStatement::Wikidata {
                                item: item.to_string(),
                            },
                        );
                    }
                }
            }
        }
        if !found.links.is_empty() {
            return Ok(found.settle());
        }

        let mut filed: Vec<(String, String)> = Vec::new();
        let mut incomplete = false;
        for key in self.discogs_counterpart_keys()? {
            let Some(json) = self.document(PayloadSource::Discogs, &key) else {
                found.unread |= self.was_unfetched(PayloadSource::Discogs, &key);
                incomplete = true;
                continue;
            };
            match crate::discogs::client::parse_discogs_release_json(json)?.master_id {
                Some(master) => filed.push((key, master)),
                None => incomplete = true,
            }
        }
        let masters: HashSet<&str> = filed.iter().map(|(_, master)| master.as_str()).collect();
        if !incomplete || masters.len() > 1 {
            for (twin, master) in &filed {
                found.push(
                    MetadataRef::new(Catalog::Discogs, master.clone()),
                    AlbumStatement::Release {
                        musicbrainz_release: self.release.key.clone(),
                        twin: MetadataRef::new(Catalog::Discogs, twin.clone()),
                    },
                );
            }
        }
        Ok(found.settle())
    }

    /// Whether the walk named this document and could not get it.
    fn was_unfetched(&self, document: PayloadSource, key: &str) -> bool {
        self.unfetched
            .iter()
            .any(|unfetched| unfetched.document == document && unfetched.key == key)
    }

    /// Resolve whole groups of equally strong claims before following their
    /// documents. Archive order and URL order cannot select an album identity.
    pub(super) fn album_links(&self) -> Result<Vec<MetadataRef>, ImportError> {
        let mut links = AlbumClaims(HashMap::new());
        links.admit(self.anchor_parent()?);
        let musicbrainz = match self.release.catalog {
            Catalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            Catalog::Discogs => self.musicbrainz_xref()?,
            other => not_fetched(other),
        };
        // A MusicBrainz release's album on the other lookup catalog is what
        // its statements name, and nothing the walk below reaches.
        let walked = |claim: &MetadataRef| {
            self.release.catalog != Catalog::MusicBrainz || !names_other_album(claim.catalog)
        };
        match self.release.catalog {
            Catalog::MusicBrainz => {
                links.admit(
                    self.album_statements()?
                        .read()
                        .iter()
                        .map(|link| link.album.clone()),
                );
                links.admit(
                    album_urls(&musicbrainz.as_ref().expect("selected MB release").relations)
                        .into_iter()
                        .filter(walked),
                );
            }
            Catalog::Discogs => {
                links.admit(self.counterpart_group()?);
                if let Some(release) = &musicbrainz {
                    links.admit(album_urls(&release.relations));
                }
            }
            other => not_fetched(other),
        }
        let mut visited = HashSet::new();
        loop {
            let pending: Vec<_> = links
                .known()
                .into_iter()
                .filter(|identity| visited.insert(identity.clone()))
                .collect();
            if pending.is_empty() {
                break;
            }
            let mut claims = Vec::new();
            for identity in pending {
                match identity.catalog {
                    Catalog::MusicBrainz => {
                        if let Some(relations) = musicbrainz
                            .as_ref()
                            .and_then(|release| release.release_group.as_ref())
                            .filter(|group| group.id == identity.key)
                            .and_then(|group| group.relations.as_ref())
                        {
                            claims.extend(album_urls(relations));
                        }
                        if let Some(json) =
                            self.document(PayloadSource::MusicBrainzReleaseGroup, &identity.key)
                        {
                            let group = crate::musicbrainz::parse_release_group(json)
                                .map_err(|error| self.source_data(error.to_string()))?;
                            claims.extend(album_urls(&group.relations));
                        }
                    }
                    Catalog::Discogs => {
                        if let Some(json) = self
                            .document(PayloadSource::MusicBrainzDiscogsMasterXref, &identity.key)
                        {
                            let group = crate::musicbrainz::parse_release_group(json)
                                .map_err(|error| self.source_data(error.to_string()))?;
                            claims.push(MetadataRef::new(Catalog::MusicBrainz, group.id));
                        }
                    }
                    Catalog::Wikidata => {
                        if let Some(json) = self.document(PayloadSource::Wikidata, &identity.key) {
                            let item = crate::wikidata::parse_entity(json)
                                .map_err(|error| self.source_data(error.to_string()))?;
                            claims.extend(item.catalog_pages().into_iter().filter_map(|page| {
                                match page {
                                    CatalogPage::Group { catalog, key } => {
                                        Some(MetadataRef::new(catalog, key))
                                    }
                                    CatalogPage::Release { .. } => None,
                                }
                            }));
                        }
                    }
                    _ => {}
                }
            }
            links.admit(claims.into_iter().filter(walked));
        }
        Ok(links.known())
    }
}
