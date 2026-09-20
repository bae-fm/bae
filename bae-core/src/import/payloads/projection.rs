use super::*;
use crate::import::release_metadata::{AlbumMetadata, ReleaseMetadata};

impl ReleasePayloads {
    pub(super) fn counterpart(&self) -> Result<Option<MetadataRef>, ImportError> {
        match self.release.catalog {
            Catalog::MusicBrainz => {
                let keys = self.discogs_counterpart_keys()?;
                Ok(match keys.as_slice() {
                    [key] => Some(MetadataRef::new(Catalog::Discogs, key)),
                    _ => None,
                })
            }
            Catalog::Discogs => self.musicbrainz_xref().map(|release| {
                release.map(|release| MetadataRef::new(Catalog::MusicBrainz, release.id))
            }),
            other => not_fetched(other),
        }
    }

    pub(super) fn anchor_parent(&self) -> Result<Option<MetadataRef>, ImportError> {
        Ok(match self.release.catalog {
            Catalog::MusicBrainz => self
                .musicbrainz_anchor()?
                .release_group
                .map(|group| MetadataRef::new(Catalog::MusicBrainz, group.id)),
            Catalog::Discogs => self
                .discogs_anchor()?
                .master_id
                .map(|key| MetadataRef::new(Catalog::Discogs, key)),
            other => not_fetched(other),
        })
    }

    /// Parents are projected in selected-provider order. Alias documents name
    /// the same entity as the canonical document and are read only once.
    pub(super) fn album_documents(&self) -> Result<Vec<(Catalog, &str, &str)>, ImportError> {
        let parent = self.anchor_parent()?;
        let admitted = self.album_links()?;
        let mut documents = Vec::new();
        for document in self.supporting.iter() {
            let catalog = match document.source {
                PayloadSource::MusicBrainzReleaseGroup => Catalog::MusicBrainz,
                PayloadSource::DiscogsMaster => Catalog::Discogs,
                _ => continue,
            };
            if !admitted.contains(&MetadataRef::new(catalog, &document.source_release_id)) {
                continue;
            }
            documents.push((
                catalog,
                document.source_release_id.as_str(),
                document.json.as_str(),
            ));
        }
        documents.sort_by_key(|(catalog, key, _)| {
            let is_parent = parent
                .as_ref()
                .is_some_and(|parent| parent.catalog == *catalog && parent.key == *key);
            (!is_parent, *catalog != self.release.catalog, *key)
        });
        Ok(documents)
    }

    pub(super) fn projected_metadata(&self) -> Result<ReleaseMetadata, ImportError> {
        let mut metadata = match self.release.catalog {
            Catalog::MusicBrainz => {
                crate::import::musicbrainz_mapper::metadata(&self.musicbrainz_anchor()?)?
            }
            Catalog::Discogs => crate::import::discogs_mapper::metadata(&self.discogs_anchor()?),
            other => not_fetched(other),
        };
        let selected_pressing_year = metadata.pressing.year;
        // A release's own album date, when present, precedes parent documents.
        let selected_album_year = metadata.album.year;
        let counterpart = match self.release.catalog {
            Catalog::MusicBrainz => self
                .discogs_xref()?
                .map(|release| crate::import::discogs_mapper::metadata(&release)),
            Catalog::Discogs => self
                .musicbrainz_xref()?
                .map(|mut release| {
                    release.artist_credit =
                        usable_supplemental_credits(release.artist_credit, &release.id)?;
                    crate::import::musicbrainz_mapper::metadata(&release)
                })
                .transpose()?,
            other => not_fetched(other),
        };
        let linked_album_year = counterpart.as_ref().and_then(|value| value.album.year);
        if let Some(mut counterpart) = counterpart {
            // Album dates are resolved with parent precedence below, separately
            // from the linked pressing's date.
            counterpart.album.year = None;
            metadata.fill_missing(counterpart);
        }
        metadata.album.year = selected_album_year;
        for (catalog, key, json) in self.album_documents()? {
            let album = match catalog {
                Catalog::MusicBrainz => {
                    let group = crate::musicbrainz::parse_release_group(json)
                        .map_err(|error| self.source_data(error.to_string()))?;
                    let credits = usable_supplemental_credits(group.artist_credit, key)?;
                    AlbumMetadata {
                        title: group.title.unwrap_or_default(),
                        artists: crate::import::musicbrainz_mapper::artist_credits(&credits, key)?,
                        year: crate::import::parse_year(group.first_release_date.as_deref()),
                    }
                }
                Catalog::Discogs => {
                    let master = crate::discogs::client::parse_discogs_master_json(json)?;
                    AlbumMetadata {
                        title: master.title.unwrap_or_default(),
                        artists: master
                            .artists
                            .iter()
                            .map(crate::import::discogs_mapper::discogs_track_artist_ref)
                            .collect(),
                        year: master.year.map(|year| year as i32),
                    }
                }
                other => not_fetched(other),
            };
            metadata.album.fill_missing(album);
        }
        metadata.album.year = metadata
            .album
            .year
            .or(linked_album_year)
            .or(selected_pressing_year)
            .or(metadata.pressing.year);
        Ok(metadata)
    }

    pub(super) fn projected_records(&self) -> Result<Vec<ReleaseRecord>, ImportError> {
        let parent = self.anchor_parent()?;
        let mut records = vec![ReleaseRecord::new(
            &self.release,
            parent.map(|parent| parent.key),
            true,
        )];
        if let Some(counterpart) = self.counterpart()? {
            let parent = match counterpart.catalog {
                Catalog::Discogs => self.discogs_xref()?.and_then(|release| release.master_id),
                Catalog::MusicBrainz => self
                    .musicbrainz_xref()?
                    .and_then(|release| release.release_group.map(|group| group.id)),
                other => not_fetched(other),
            };
            merge_record(
                &mut records,
                ReleaseRecord::new(&counterpart, parent, false),
            );
        }
        for album in self.album_links()? {
            merge_record(&mut records, ReleaseRecord::album(&album));
        }
        records.sort_by_key(|record| {
            Catalog::ALL
                .iter()
                .position(|catalog| *catalog == record.catalog())
                .expect("known catalog")
        });
        Ok(records)
    }
}

fn merge_record(records: &mut Vec<ReleaseRecord>, incoming: ReleaseRecord) {
    match records
        .iter_mut()
        .find(|record| record.catalog() == incoming.catalog())
    {
        Some(existing @ ReleaseRecord::Album { .. })
            if matches!(incoming, ReleaseRecord::Pressing { .. }) =>
        {
            *existing = incoming
        }
        Some(_) => {}
        None => records.push(incoming),
    }
}

/// Supplemental credits may be unusable without invalidating the selected
/// release or the other facts the linked document supplies. Keep the selected
/// release's strict validation in its mapper.
fn usable_supplemental_credits(
    credits: Vec<crate::musicbrainz::MbArtistCredit>,
    entity_id: &str,
) -> Result<Vec<crate::musicbrainz::MbArtistCredit>, ImportError> {
    let mut usable = Vec::new();
    for credit in credits {
        match crate::import::musicbrainz_mapper::artist_credits(
            std::slice::from_ref(&credit),
            entity_id,
        ) {
            Ok(_) => usable.push(credit),
            Err(error @ ImportError::SourceData { .. }) => {
                warn!(
                    entity_id,
                    %error,
                    "Skipping unusable supplemental MusicBrainz artist credit"
                );
            }
            Err(error) => return Err(error),
        }
    }
    Ok(usable)
}
