//! A Discogs release's `formats`, read into bae's pressing facts.
//!
//! Discogs states a release's media as format entries: a name from its
//! format list, a quantity, and descriptions from its description list. One
//! reading turns them into every fact they carry — the media, the status a
//! "Promo" states, the box a "Box Set" is, and the details no field holds —
//! so the search row, the stored release and the pairing evidence read the
//! same thing.

use super::discogs_detail::{self, Role};
use super::medium::DiscogsFormatName;
use super::stated_media::{StatedFormat, StatedMedia};
use super::{DiscogsDetail, Medium, Packaging, ReleaseStatus};
use crate::discogs::DiscogsFormat;
use tracing::{debug, warn};

/// Every fact a release's format entries state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormatsReading {
    pub(crate) media: StatedMedia,
    pub(crate) status: Option<ReleaseStatus>,
    pub(crate) packaging: Option<Packaging>,
    pub(crate) details: Vec<DiscogsDetail>,
}

/// Read a Discogs release's format entries. `release_id` names the release
/// in what is logged: a name or description outside Discogs's lists is kept
/// out of the facts, and the list is what needs it.
pub(crate) fn read(release_id: &str, formats: &[DiscogsFormat]) -> FormatsReading {
    let mut media = Vec::new();
    let mut status = None;
    let mut packaging = None;
    let mut details: Vec<DiscogsDetail> = Vec::new();
    // An entry with a blank name states nothing.
    for format in formats.iter().filter(|format| !format.name.trim().is_empty()) {
        let roles: Vec<Role> = format
            .descriptions
            .iter()
            .filter_map(|description| {
                let role = discogs_detail::role(description);
                if role.is_none() {
                    warn!(
                        discogs_release_id = release_id,
                        description = description.as_str(),
                        "a Discogs format description is outside Discogs's description list"
                    );
                }
                role
            })
            .collect();
        let name = Medium::discogs(&format.name);
        match name {
            Some(DiscogsFormatName::Carrier(medium)) => media.push(StatedFormat {
                medium: Some(medium),
                quantity: quantity(release_id, format),
            }),
            // A hybrid disc's kind is a description of its entry.
            Some(DiscogsFormatName::Hybrid) => media.push(StatedFormat {
                medium: roles.iter().find_map(|role| match role {
                    Role::HybridCarrier(medium) => Some(*medium),
                    _ => None,
                }),
                quantity: quantity(release_id, format),
            }),
            Some(DiscogsFormatName::BoxSet) => packaging = Some(Packaging::Box),
            Some(DiscogsFormatName::AllMedia) => {}
            None => {
                warn!(
                    discogs_release_id = release_id,
                    format = format.name.as_str(),
                    "a Discogs format name is outside Discogs's format list"
                );
                media.push(StatedFormat {
                    medium: None,
                    quantity: quantity(release_id, format),
                });
            }
        }
        for role in roles {
            match role {
                Role::Status(stated) => status = Some(stricter(status, stated)),
                Role::Detail(detail) => {
                    if !details.contains(&detail) {
                        details.push(detail);
                    }
                }
                // What the release group states, and a hybrid disc's kind,
                // which its entry has already read.
                Role::ReleaseType | Role::HybridCarrier(_) => {}
            }
        }
    }
    FormatsReading {
        media: if media.is_empty() {
            StatedMedia::Undescribed
        } else {
            StatedMedia::Formats(media)
        },
        status,
        packaging,
        details,
    }
}

/// The quantity a format entry states. Discogs writes it as a count of one
/// or more — and, on a few hundred of its millions of entries, as zero.
/// Anything but a count is read as one, since the entry still names a medium
/// the release holds.
fn quantity(release_id: &str, format: &DiscogsFormat) -> u32 {
    match format.qty.parse::<u32>() {
        Ok(quantity) if quantity > 0 => quantity,
        _ => {
            debug!(
                discogs_release_id = release_id,
                format = format.name.as_str(),
                qty = format.qty.as_str(),
                "a Discogs format entry states no count of one or more"
            );
            1
        }
    }
}

/// The status of a release two of whose entries state one each. An
/// unsanctioned promotional copy is a bootleg first: MusicBrainz has one
/// status per release, and whether the artist stands behind it is the one
/// that says more.
fn stricter(current: Option<ReleaseStatus>, stated: ReleaseStatus) -> ReleaseStatus {
    match (current, stated) {
        (Some(ReleaseStatus::Bootleg), _) | (_, ReleaseStatus::Bootleg) => ReleaseStatus::Bootleg,
        (_, stated) => stated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(name: &str, qty: &str, descriptions: &[&str]) -> DiscogsFormat {
        DiscogsFormat {
            name: name.to_string(),
            qty: qty.to_string(),
            descriptions: descriptions.iter().map(|d| d.to_string()).collect(),
        }
    }

    /// A box of records modelled on a real one: two single LPs, a four-LP
    /// set and the box they are sold in.
    #[test]
    fn a_boxed_set_reads_as_its_records_its_box_and_its_details() {
        let reading = read(
            "1",
            &[
                format("Vinyl", "1", &["LP", "Album", "Reissue", "Remastered"]),
                format("Vinyl", "1", &["LP", "Album", "Reissue", "Remastered"]),
                format("Vinyl", "4", &["LP", "Album"]),
                format("Box Set", "1", &["Compilation", "Deluxe Edition"]),
            ],
        );
        assert_eq!(
            reading.media.counts(),
            vec![crate::pressing::MediaCount {
                medium: Medium::Vinyl,
                count: 6
            }]
        );
        assert_eq!(reading.packaging, Some(Packaging::Box));
        assert_eq!(reading.status, None);
        assert_eq!(
            reading.details,
            vec![
                DiscogsDetail::Lp,
                DiscogsDetail::Reissue,
                DiscogsDetail::Remastered,
                DiscogsDetail::DeluxeEdition,
            ],
            "release types are dropped and each detail is kept once"
        );
    }

    #[test]
    fn a_promotional_copy_states_its_status() {
        let reading = read("1", &[format("CD", "1", &["Album", "Promo", "Stereo"])]);
        assert_eq!(reading.status, Some(ReleaseStatus::Promotion));
        assert_eq!(reading.details, vec![DiscogsDetail::Stereo]);
    }

    #[test]
    fn an_unofficial_promotional_copy_is_a_bootleg() {
        let reading = read(
            "1",
            &[format("Vinyl", "1", &["12\"", "Promo", "Unofficial Release"])],
        );
        assert_eq!(reading.status, Some(ReleaseStatus::Bootleg));
    }

    #[test]
    fn a_hybrid_disc_is_the_kind_its_description_names() {
        let reading = read("1", &[format("Hybrid", "1", &["DualDisc", "Album"])]);
        assert_eq!(
            reading.media,
            StatedMedia::Formats(vec![StatedFormat {
                medium: Some(Medium::DualDisc),
                quantity: 1
            }])
        );
        let unstated = read("1", &[format("Hybrid", "1", &["Album"])]);
        assert_eq!(
            unstated.media,
            StatedMedia::Formats(vec![StatedFormat {
                medium: None,
                quantity: 1
            }])
        );
    }

    #[test]
    fn words_outside_the_lists_are_left_out() {
        let reading = read(
            "1",
            &[format("Zorblax", "0", &["Reissue", "180 Gram"])],
        );
        assert_eq!(
            reading.media,
            StatedMedia::Formats(vec![StatedFormat {
                medium: None,
                quantity: 1
            }])
        );
        assert_eq!(reading.details, vec![DiscogsDetail::Reissue]);
        assert_eq!(read("1", &[]).media, StatedMedia::Undescribed);
    }
}
