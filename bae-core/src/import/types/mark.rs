//! The names an object carries, read off its folder.
//!
//! A barcode from a scan, a catalog number from the folder name, a disc ID
//! from a rip log's table of contents: each is a *mark* — something printed on
//! or derivable from the object itself, as opposed to what a catalog says
//! about it, which is a record. Extraction reads them as signals; the import
//! commit keeps them with the release so the folder's evidence survives it.

use crate::signals::{SignalOrigin, SourcedValue};
use serde::{Deserialize, Serialize};

/// Which name a mark is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkKind {
    DiscId,
    Barcode,
    CatalogNumber,
}

impl MarkKind {
    /// Every kind, in the order surfaces list them: the disc's own identity
    /// first, then the two codes printed on the package.
    pub const ALL: [MarkKind; 3] = [Self::DiscId, Self::Barcode, Self::CatalogNumber];

    /// The stored `kind` column value.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DiscId => "disc_id",
            Self::Barcode => "barcode",
            Self::CatalogNumber => "catalog_number",
        }
    }
}

impl std::str::FromStr for MarkKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == s)
            .ok_or_else(|| format!("unknown mark kind: {s}"))
    }
}

impl std::fmt::Display for MarkKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One reading of a mark: which name it is, what it said, and where it was
/// read.
///
/// One stored row per reading — the same barcode read off two scans is two of
/// these, each naming its own file and the box the detector drew around it.
/// What a surface draws is [`ReleaseMarkLine`], which folds the readings of
/// one value together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseMark {
    pub kind: MarkKind,
    pub sighting: SourcedValue,
}

/// One name the object carries, as a surface draws it: the kind, the value,
/// and every surface it was read from.
///
/// Folded here rather than by each UI: two scans showing one barcode are one
/// line tagged `scan`, and deciding that twice is deciding it twice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseMarkLine {
    pub kind: MarkKind,
    pub value: String,
    /// Every surface this value was read from, each named once, in the order
    /// it was first read from them.
    pub origins: Vec<SignalOrigin>,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ReleaseMark {
    /// Every mark one extraction pass read: the disc ID derived from a LOG or
    /// CUE, every barcode sighting, and every catalog-number sighting.
    ///
    /// Only what was read off the folder. What a catalog says the barcode is
    /// lives in that catalog's record.
    pub fn of_signals(signals: &crate::signals::Signals) -> Vec<Self> {
        let mut marks = Vec::new();
        if let crate::signals::DiscIdSignal::Computed {
            disc_id,
            source_file,
            ..
        } = &signals.disc_id
        {
            marks.push(Self {
                kind: MarkKind::DiscId,
                sighting: SourcedValue {
                    value: disc_id.clone(),
                    // A disc ID is derived from the table of contents; there is
                    // no other surface it can be read off.
                    origin: SignalOrigin::DiscToc,
                    origin_path: source_file.clone(),
                    region: None,
                },
            });
        }
        marks.extend(signals.barcode.codes().iter().map(|sighting| Self {
            kind: MarkKind::Barcode,
            sighting: sighting.clone(),
        }));
        marks.extend(signals.text.catalogs().iter().map(|sighting| Self {
            kind: MarkKind::CatalogNumber,
            sighting: sighting.clone(),
        }));
        marks
    }
}

impl ReleaseMarkLine {
    /// The lines `marks` draw as: one per value, in [`MarkKind::ALL`] order
    /// and, within a kind, in the order the values were first read.
    pub fn fold(marks: &[ReleaseMark]) -> Vec<Self> {
        let mut lines: Vec<Self> = Vec::new();
        for kind in MarkKind::ALL {
            for mark in marks.iter().filter(|mark| mark.kind == kind) {
                match lines
                    .iter_mut()
                    .find(|line| line.kind == kind && line.value == mark.sighting.value)
                {
                    Some(line) => {
                        if !line.origins.contains(&mark.sighting.origin) {
                            line.origins.push(mark.sighting.origin);
                        }
                    }
                    None => lines.push(Self {
                        kind,
                        value: mark.sighting.value.clone(),
                        origins: vec![mark.sighting.origin],
                    }),
                }
            }
        }
        lines
    }
}

#[cfg(all(test, not(any(target_os = "ios", target_os = "android"))))]
mod tests {
    use super::*;
    use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

    fn region() -> Option<crate::signals::ImageRegion> {
        crate::signals::ImageRegion::new(0.1, 0.2, 0.3, 0.4)
    }

    /// One folder's whole reading: a disc ID derived from its log, the same
    /// barcode printed on two of its scans, and a catalog number in its name.
    fn signals() -> Signals {
        Signals {
            disc_id: DiscIdSignal::Computed {
                disc_id: "XyZ.abc-123".to_string(),
                track_count: 11,
                source_file: Some("Album.log".to_string()),
            },
            barcode: BarcodeSignal::Settled {
                codes: vec![
                    SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    )
                    .at(region()),
                    SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::CueSheet,
                        "Album.cue".to_string(),
                    ),
                ],
            },
            text: TextSignal::Settled {
                catalogs: vec![SourcedValue::new(
                    "7559-60691-2".to_string(),
                    SignalOrigin::FolderName,
                )],
                free_text: vec!["Album Title".to_string()],
            },
            text_pool: Vec::new(),
            durations: crate::import::probe::SourceDurations::default(),
        }
    }

    /// Every name the folder states becomes a sighting, and each keeps where
    /// it was read: the disc ID is the table of contents' whatever file
    /// carried it, and the barcode read off a scan keeps the box it was read
    /// in. The free text is not a name the object carries, so nothing of it
    /// survives here.
    #[test]
    fn every_name_the_folder_states_becomes_a_sighting() {
        assert_eq!(
            ReleaseMark::of_signals(&signals()),
            vec![
                ReleaseMark {
                    kind: MarkKind::DiscId,
                    sighting: SourcedValue::in_file(
                        "XyZ.abc-123".to_string(),
                        SignalOrigin::DiscToc,
                        "Album.log".to_string(),
                    ),
                },
                ReleaseMark {
                    kind: MarkKind::Barcode,
                    sighting: SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    )
                    .at(region()),
                },
                ReleaseMark {
                    kind: MarkKind::Barcode,
                    sighting: SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::CueSheet,
                        "Album.cue".to_string(),
                    ),
                },
                ReleaseMark {
                    kind: MarkKind::CatalogNumber,
                    sighting: SourcedValue::new(
                        "7559-60691-2".to_string(),
                        SignalOrigin::FolderName,
                    ),
                },
            ],
        );
    }

    /// The two sightings of one barcode draw one line, tagged with both
    /// surfaces it was read from; the lines come in kind order.
    #[test]
    fn the_sightings_of_one_value_draw_one_line() {
        assert_eq!(
            ReleaseMarkLine::fold(&ReleaseMark::of_signals(&signals())),
            vec![
                ReleaseMarkLine {
                    kind: MarkKind::DiscId,
                    value: "XyZ.abc-123".to_string(),
                    origins: vec![SignalOrigin::DiscToc],
                },
                ReleaseMarkLine {
                    kind: MarkKind::Barcode,
                    value: "0075678164521".to_string(),
                    origins: vec![SignalOrigin::Artwork, SignalOrigin::CueSheet],
                },
                ReleaseMarkLine {
                    kind: MarkKind::CatalogNumber,
                    value: "7559-60691-2".to_string(),
                    origins: vec![SignalOrigin::FolderName],
                },
            ],
        );
    }

    /// Two scans of the same cover state the barcode twice; the line names the
    /// surface once, because the tag says where a value was read, not how
    /// often.
    #[test]
    fn one_surface_is_named_once_however_many_times_it_stated_a_value() {
        let twice = vec![
            ReleaseMark {
                kind: MarkKind::Barcode,
                sighting: SourcedValue::in_file(
                    "0075678164521".to_string(),
                    SignalOrigin::Artwork,
                    "back.jpg".to_string(),
                ),
            },
            ReleaseMark {
                kind: MarkKind::Barcode,
                sighting: SourcedValue::in_file(
                    "0075678164521".to_string(),
                    SignalOrigin::Artwork,
                    "back-2.jpg".to_string(),
                ),
            },
        ];
        assert_eq!(
            ReleaseMarkLine::fold(&twice),
            vec![ReleaseMarkLine {
                kind: MarkKind::Barcode,
                value: "0075678164521".to_string(),
                origins: vec![SignalOrigin::Artwork],
            }],
        );
    }

    /// A folder nothing was read off states no names.
    #[test]
    fn a_folder_that_stated_nothing_marks_nothing() {
        let silent = Signals {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            text_pool: Vec::new(),
            durations: crate::import::probe::SourceDurations::default(),
        };
        assert!(ReleaseMark::of_signals(&silent).is_empty());
    }

    /// The stored word and the kind read back from it are one mapping.
    #[test]
    fn every_kind_round_trips_its_stored_word() {
        for kind in MarkKind::ALL {
            assert_eq!(kind.as_str().parse::<MarkKind>(), Ok(kind));
        }
        assert!("matrix".parse::<MarkKind>().is_err());
    }
}
