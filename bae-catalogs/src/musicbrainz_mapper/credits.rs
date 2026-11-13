//! The credits and works a MusicBrainz release states: who each track and the
//! release credit, and the composed works a recording performs.

use super::*;
use bae_types::credit_role::musicbrainz::{credits, Attributes, Reading, Subject};
use bae_types::credit_role::CreditedAs;

pub(super) fn mb_relation_is(
    relation: &MbRelation,
    target_type: &str,
    relation_type: &str,
) -> bool {
    relation.target_type.as_deref() == Some(target_type)
        && relation.relation_type.as_deref() == Some(relation_type)
}

/// The artist a MusicBrainz credit names, by the artist's own name (else the
/// name the credit prints), with the printed name kept where it is another.
/// `None` where neither names anyone.
pub(super) fn mb_credited_artist(artist: &MbArtistRef, printed: &str) -> Option<ArtistCredit> {
    let name = artist
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .or(Some(printed.trim()).filter(|printed| !printed.is_empty()))?
        .to_string();
    Some(
        ArtistCredit {
            credited_as: None,
            name,
            sort_name: artist.sort_name.clone(),
            musicbrainz_artist_id: artist.id.clone(),
            discogs_artist_id: None,
        }
        .printed_as(Some(printed)),
    )
}

/// The artist a MusicBrainz relation credits: by the artist's own name,
/// printing the name the relation credits (`target-credit`) where that is
/// another. `None` where neither names anyone.
pub(super) fn mb_relation_artist(
    relation: &MbRelation,
    artist: &MbArtistRef,
) -> Option<ArtistCredit> {
    mb_credited_artist(artist, relation.target_credit.as_deref().unwrap_or_default())
}

/// The credits an artist relation to `subject` states, each with the artist
/// it credits: one for each instrument or voice a performer relation names,
/// none for a relation that is malformed or credits no work. A relation
/// dropped is logged, naming `on` (the thing it relates the artist to) unless
/// `report` is off, for a work reached from several tracks.
pub(super) fn artist_relation_credits(
    subject: Subject,
    on: &str,
    relation: &MbRelation,
    report: bool,
) -> Vec<(ArtistCredit, CreditedAs)> {
    let Some(artist_ref) = relation.artist.as_ref() else {
        if report {
            warn!(
                on,
                relation_type = ?relation.relation_type,
                "Skipping MusicBrainz artist relation without artist payload"
            );
        }
        return Vec::new();
    };
    let Some(relation_type) = relation.relation_type.as_deref() else {
        if report {
            warn!(
                on,
                musicbrainz_artist_id = ?artist_ref.id,
                "Skipping MusicBrainz artist relation without a type"
            );
        }
        return Vec::new();
    };
    let Some(artist) = mb_relation_artist(relation, artist_ref) else {
        if report {
            warn!(
                on,
                musicbrainz_artist_id = ?artist_ref.id,
                "Skipping MusicBrainz artist relation with unresolved artist"
            );
        }
        return Vec::new();
    };
    let stated = match credits(
        subject,
        relation_type,
        Attributes {
            names: &relation.attributes,
            ids: &relation.attribute_ids,
            credits: &relation.attribute_credits,
            values: &relation.attribute_values,
        },
    ) {
        Reading::Credits(stated) => stated,
        Reading::Unlisted(stated) => {
            if report {
                warn!(
                    on,
                    relation_type,
                    "MusicBrainz artist relation of a type bae's list does not hold; credited in the type's own name"
                );
            }
            vec![stated]
        }
        Reading::NotACredit => {
            if report {
                debug!(
                    on,
                    relation_type,
                    target_credit = ?relation.target_credit,
                    "Skipping MusicBrainz artist relation that credits no work"
                );
            }
            Vec::new()
        }
    };
    stated.into_iter().map(|role| (artist.clone(), role)).collect()
}

/// Read an `MbWork` as one reference states it, validating its relations:
/// malformed ones are dropped here, at the source boundary, so a stored work
/// only ever holds events that carry their payloads.
///
/// `reported` is release-scoped: a work reached from several tracks logs its
/// dropped relations the first time it is read, not once per reference.
pub(super) fn source_work(work: &MbWork, reported: &mut HashSet<String>) -> SourceWork {
    let report = reported.insert(work.id.clone());
    let mut events = Vec::new();
    for relation in &work.relations {
        if relation.target_type.as_deref() == Some("artist") {
            for (artist, role) in artist_relation_credits(Subject::Work, &work.id, relation, report)
            {
                events.push(SourceWorkEvent::Credit { artist, role });
            }
        } else if mb_relation_is(relation, "work", "parts") {
            let Some(child_or_parent) = relation.work.as_ref() else {
                if report {
                    warn!(
                        work_id = %work.id,
                        relation_type = ?relation.relation_type,
                        "Skipping MusicBrainz work parts relation without work payload"
                    );
                }
                continue;
            };
            let direction = match relation.direction.as_deref() {
                Some("backward") => PartDirection::Backward,
                _ => PartDirection::Forward,
            };
            events.push(SourceWorkEvent::Part {
                direction,
                work: source_work(child_or_parent, reported),
            });
        }
    }
    SourceWork {
        musicbrainz_work_id: work.id.clone(),
        title: work.title.clone(),
        disambiguation: work.disambiguation.clone(),
        work_type: work.work_type.clone(),
        events,
    }
}

/// Add `artist` in `role` to `roles` at the next position, unless the same
/// artist is already credited in the same role: MusicBrainz states one
/// instrument both alone and with others, and a credit is one artist in one
/// role.
pub(super) fn push_role(roles: &mut Vec<RoleCredit>, artist: ArtistCredit, role: CreditedAs) {
    if !roles.iter().any(|held| held.artist == artist && held.role == role) {
        roles.push(RoleCredit { position: roles.len() as i32, artist, role });
    }
}

/// A release's or track's MusicBrainz artist credit, place by place, with
/// what MusicBrainz prints after each. A credit naming no artist object keeps
/// its printed name as a credit of its own; the last credit's join is not
/// kept, since nothing follows it.
pub(crate) fn artist_credits(
    credits: &[crate::musicbrainz::MbArtistCredit],
    entity_id: &str,
) -> Result<Vec<SourceCredit>, CatalogError> {
    let mut stated: Vec<SourceCredit> = Vec::new();
    for credit in credits {
        let artist = match &credit.artist {
            Some(artist_obj) => mb_credited_artist(artist_obj, &credit.name).ok_or_else(|| {
                CatalogError::SourceData {
                    catalog: Catalog::MusicBrainz,
                    detail: format!(
                        "MusicBrainz release {} artist credit {:?} has no artist name",
                        entity_id, artist_obj.id
                    ),
                }
            })?,
            None if credit.name.trim().is_empty() => continue,
            None => ArtistCredit::named(credit.name.trim()),
        };
        stated.push(SourceCredit {
            credit: artist,
            join: bae_types::credits::stated_join(credit.joinphrase.as_deref()),
        });
    }
    bae_types::credits::drop_last_join(&mut stated, |credit| &mut credit.join);
    Ok(stated)
}

#[cfg(test)]
#[path = "credits_tests.rs"]
mod tests;
