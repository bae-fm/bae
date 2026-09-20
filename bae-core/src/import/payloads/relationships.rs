use super::*;
use std::collections::{HashMap, HashSet};

/// The strongest album claims encountered while following this release's links.
/// An ambiguous claim occupies its catalog so weaker links cannot choose for it.
struct AlbumLinks(HashMap<Catalog, AlbumIdentity>);

enum AlbumIdentity {
    Known(String),
    Ambiguous,
}

impl AlbumLinks {
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

    /// Multiple pressing links establish an album only when every named release
    /// resolves and independently states that same parent.
    fn counterpart_parents(&self) -> Result<Vec<MetadataRef>, ImportError> {
        match self.release.catalog {
            Catalog::Discogs => Ok(self
                .musicbrainz_xref()?
                .and_then(|release| {
                    release
                        .release_group
                        .map(|group| MetadataRef::new(Catalog::MusicBrainz, group.id))
                })
                .into_iter()
                .collect()),
            Catalog::MusicBrainz => {
                let keys = self.discogs_counterpart_keys()?;
                let mut parents = Vec::new();
                let mut incomplete = false;
                for key in &keys {
                    let Some(json) = self.document(PayloadSource::Discogs, key) else {
                        incomplete = true;
                        continue;
                    };
                    let Some(parent) =
                        crate::discogs::client::parse_discogs_release_json(json)?.master_id
                    else {
                        incomplete = true;
                        continue;
                    };
                    parents.push(parent);
                }
                if incomplete && parents.iter().collect::<HashSet<_>>().len() < 2 {
                    return Ok(Vec::new());
                }
                Ok(parents
                    .into_iter()
                    .map(|key| MetadataRef::new(Catalog::Discogs, key))
                    .collect())
            }
            other => not_fetched(other),
        }
    }

    /// Resolve whole groups of equally strong claims before following their
    /// documents. Archive order and URL order cannot select an album identity.
    pub(super) fn album_links(&self) -> Result<Vec<MetadataRef>, ImportError> {
        let mut links = AlbumLinks(HashMap::new());
        links.admit(self.anchor_parent()?);
        let musicbrainz = match self.release.catalog {
            Catalog::MusicBrainz => Some(self.musicbrainz_anchor()?),
            Catalog::Discogs => self.musicbrainz_xref()?,
            other => not_fetched(other),
        };
        if self.release.catalog == Catalog::MusicBrainz {
            links.admit(album_urls(
                &musicbrainz.as_ref().expect("selected MB release").relations,
            ));
        }
        links.admit(self.counterpart_parents()?);
        if self.release.catalog == Catalog::Discogs {
            if let Some(release) = &musicbrainz {
                links.admit(album_urls(&release.relations));
            }
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
            links.admit(claims);
        }
        Ok(links.known())
    }
}
