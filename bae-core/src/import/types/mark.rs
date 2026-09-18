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

    /// Catalog spellings ignore punctuation and case; opaque IDs and barcode
    /// digits must agree exactly before their readings share evidence.
    pub(crate) fn same_value(self, left: &str, right: &str) -> bool {
        match self {
            Self::CatalogNumber => {
                crate::util::text::squash(left) == crate::util::text::squash(right)
            }
            Self::DiscId | Self::Barcode => left == right,
        }
    }

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
    /// This value's lookup named the record chosen for the release.
    pub corroborated: bool,
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
    pub corroborated: bool,
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
impl ReleaseMark {
    /// The names one extraction pass read that the person let the run ask
    /// about: the disc ID derived from a LOG or CUE unless it is left out,
    /// every sighting of a barcode that is not left out, and every sighting of
    /// a chosen catalog number, the numbers in the order they were chosen.
    ///
    /// Extraction's catalog-number pool is guesses — OCR off a scan reads a
    /// date and a misread of the number beside the number itself — so a number
    /// is a name the object carries once somebody says it is, which is the
    /// same thing that decides whether the run looks it up.
    ///
    /// Only what was read off the folder. What a catalog says the barcode is
    /// lives in that catalog's record.
    pub fn of_signals(
        signals: &crate::signals::Signals,
        choices: &crate::import::LookupChoices,
    ) -> Vec<Self> {
        let mut marks = Vec::new();
        if let crate::signals::DiscIdSignal::Computed {
            disc_id,
            source_file,
            ..
        } = &signals.disc_id
        {
            if !choices.disc_id_excluded {
                marks.push(Self {
                    kind: MarkKind::DiscId,
                    corroborated: false,
                    sighting: SourcedValue {
                        value: disc_id.clone(),
                        // A disc ID is derived from the table of contents; there
                        // is no other surface it can be read off.
                        origin: SignalOrigin::DiscToc,
                        origin_path: source_file.clone(),
                        region: None,
                    },
                });
            }
        }
        marks.extend(
            signals
                .barcode
                .codes()
                .iter()
                .filter(|sighting| !choices.excluded_barcodes.contains(&sighting.value))
                .map(|sighting| Self {
                    kind: MarkKind::Barcode,
                    corroborated: false,
                    sighting: sighting.clone(),
                }),
        );
        // Every sighting of a chosen number, so the surfaces that stated it
        // still fold into one line's tags. Matched as the text is searched —
        // punctuation and case dropped — so `NJ-8255` printed on the sleeve
        // and `NJ 8255` chosen off the record are one number.
        marks.extend(choices.chosen_catalogs.iter().flat_map(|chosen| {
            let chosen = crate::util::text::squash(chosen);
            signals
                .text
                .catalogs()
                .iter()
                .filter(move |sighting| crate::util::text::squash(&sighting.value) == chosen)
                .map(|sighting| Self {
                    kind: MarkKind::CatalogNumber,
                    corroborated: false,
                    sighting: sighting.clone(),
                })
        }));
        marks
    }
}

impl ReleaseMarkLine {
    /// The lines `marks` draw as: one per value, in [`MarkKind::ALL`] order
    /// and, within a kind, in the order the values were first read.
    ///
    /// Catalog values ignore punctuation and case — `NJ-8255` on
    /// the folder and `NJ 8255` on the scan are one line, spelled as it was
    /// first read — because that is how the text is searched for it and how
    /// a chosen number gathers its sightings.
    pub fn fold(marks: &[ReleaseMark]) -> Vec<Self> {
        let mut lines: Vec<Self> = Vec::new();
        for kind in MarkKind::ALL {
            for mark in marks.iter().filter(|mark| mark.kind == kind) {
                match lines.iter_mut().find(|line| {
                    line.kind == kind && kind.same_value(&line.value, &mark.sighting.value)
                }) {
                    Some(line) => {
                        line.corroborated |= mark.corroborated;
                        if !line.origins.contains(&mark.sighting.origin) {
                            line.origins.push(mark.sighting.origin);
                        }
                    }
                    None => lines.push(Self {
                        kind,
                        corroborated: mark.corroborated,
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
    use crate::import::LookupChoices;
    use crate::signals::{BarcodeSignal, DiscIdSignal, Signals, TextSignal};

    fn region() -> Option<crate::signals::ImageRegion> {
        crate::signals::ImageRegion::new(0.1, 0.2, 0.3, 0.4)
    }

    /// Choices whose only decision is which catalog numbers the run asks
    /// about.
    fn choosing(catalogs: &[&str]) -> LookupChoices {
        LookupChoices {
            chosen_catalogs: catalogs.iter().map(|value| value.to_string()).collect(),
            ..LookupChoices::default()
        }
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
            verification: None,
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

    /// Every name the person let the run ask about becomes a sighting, and
    /// each keeps where it was read: the disc ID is the table of contents'
    /// whatever file carried it, and the barcode read off a scan keeps the box
    /// it was read in. The free text is not a name the object carries, so
    /// nothing of it survives here.
    #[test]
    fn every_name_the_run_asks_about_becomes_a_sighting() {
        assert_eq!(
            ReleaseMark::of_signals(&signals(), &choosing(&["7559-60691-2"])),
            vec![
                ReleaseMark {
                    corroborated: false,
                    kind: MarkKind::DiscId,
                    sighting: SourcedValue::in_file(
                        "XyZ.abc-123".to_string(),
                        SignalOrigin::DiscToc,
                        "Album.log".to_string(),
                    ),
                },
                ReleaseMark {
                    corroborated: false,
                    kind: MarkKind::Barcode,
                    sighting: SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    )
                    .at(region()),
                },
                ReleaseMark {
                    corroborated: false,
                    kind: MarkKind::Barcode,
                    sighting: SourcedValue::in_file(
                        "0075678164521".to_string(),
                        SignalOrigin::CueSheet,
                        "Album.cue".to_string(),
                    ),
                },
                ReleaseMark {
                    corroborated: false,
                    kind: MarkKind::CatalogNumber,
                    sighting: SourcedValue::new(
                        "7559-60691-2".to_string(),
                        SignalOrigin::FolderName,
                    ),
                },
            ],
        );
    }

    #[test]
    fn distinct_disc_ids_cannot_share_a_seal() {
        let marks = [("aBc-1", false), ("abc-1", true)].map(|(value, corroborated)| ReleaseMark {
            kind: MarkKind::DiscId,
            sighting: SourcedValue::new(value.to_string(), SignalOrigin::DiscToc),
            corroborated,
        });
        let lines = ReleaseMarkLine::fold(&marks);
        assert_eq!(lines.len(), 2);
        assert!(!lines[0].corroborated);
        assert!(lines[1].corroborated);
    }

    /// The two sightings of one barcode draw one line, tagged with both
    /// surfaces it was read from; the lines come in kind order.
    #[test]
    fn the_sightings_of_one_value_draw_one_line() {
        assert_eq!(
            ReleaseMarkLine::fold(&ReleaseMark::of_signals(
                &signals(),
                &choosing(&["7559-60691-2"]),
            )),
            vec![
                ReleaseMarkLine {
                    corroborated: false,
                    kind: MarkKind::DiscId,
                    value: "XyZ.abc-123".to_string(),
                    origins: vec![SignalOrigin::DiscToc],
                },
                ReleaseMarkLine {
                    corroborated: false,
                    kind: MarkKind::Barcode,
                    value: "0075678164521".to_string(),
                    origins: vec![SignalOrigin::Artwork, SignalOrigin::CueSheet],
                },
                ReleaseMarkLine {
                    corroborated: false,
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
                corroborated: false,
                kind: MarkKind::Barcode,
                sighting: SourcedValue::in_file(
                    "0075678164521".to_string(),
                    SignalOrigin::Artwork,
                    "back.jpg".to_string(),
                ),
            },
            ReleaseMark {
                corroborated: false,
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
                corroborated: false,
                kind: MarkKind::Barcode,
                value: "0075678164521".to_string(),
                origins: vec![SignalOrigin::Artwork],
            }],
        );
    }

    /// A pool is what extraction guessed — the number printed on the disc
    /// beside a date and a misread of it — so only the number somebody said
    /// this disc carries is a mark, and every surface that stated it is kept.
    #[test]
    fn only_the_chosen_number_of_a_pool_is_a_name() {
        let pooled = Signals {
            text: TextSignal::Settled {
                catalogs: vec![
                    SourcedValue::new("JUNE 2000".to_string(), SignalOrigin::Artwork),
                    SourcedValue::new("RISECD073".to_string(), SignalOrigin::FolderName),
                    SourcedValue::in_file(
                        "RISECD073".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    ),
                    SourcedValue::new("BECD073".to_string(), SignalOrigin::Artwork),
                ],
                free_text: Vec::new(),
            },
            barcode: BarcodeSignal::Absent,
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            ..signals()
        };
        assert_eq!(
            ReleaseMark::of_signals(&pooled, &choosing(&["RISECD073"]))
                .into_iter()
                .filter(|mark| mark.kind == MarkKind::CatalogNumber)
                .collect::<Vec<_>>(),
            vec![
                ReleaseMark {
                    corroborated: false,
                    kind: MarkKind::CatalogNumber,
                    sighting: SourcedValue::new("RISECD073".to_string(), SignalOrigin::FolderName),
                },
                ReleaseMark {
                    corroborated: false,
                    kind: MarkKind::CatalogNumber,
                    sighting: SourcedValue::in_file(
                        "RISECD073".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    ),
                },
            ],
        );
        assert_eq!(
            ReleaseMarkLine::fold(&ReleaseMark::of_signals(&pooled, &choosing(&["RISECD073"]))),
            vec![ReleaseMarkLine {
                corroborated: false,
                kind: MarkKind::CatalogNumber,
                value: "RISECD073".to_string(),
                origins: vec![SignalOrigin::FolderName, SignalOrigin::Artwork],
            }],
        );
        assert!(
            ReleaseMark::of_signals(&pooled, &LookupChoices::default())
                .iter()
                .all(|mark| mark.kind != MarkKind::CatalogNumber),
            "a folder nobody has decided about carries no catalog number"
        );
    }

    /// A number is chosen as the record spells it and printed as the sleeve
    /// does; both spellings are one number, so a chosen `NJ 8255` folds the
    /// `NJ-8255` the folder and the scan state into one line.
    #[test]
    fn a_chosen_number_folds_its_sightings_however_they_are_punctuated() {
        let spelled_two_ways = Signals {
            text: TextSignal::Settled {
                catalogs: vec![
                    SourcedValue::new("NJ-8255".to_string(), SignalOrigin::FolderName),
                    SourcedValue::in_file(
                        "NJ 8255".to_string(),
                        SignalOrigin::Artwork,
                        "back.jpg".to_string(),
                    ),
                ],
                free_text: Vec::new(),
            },
            barcode: BarcodeSignal::Absent,
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            ..signals()
        };
        assert_eq!(
            ReleaseMarkLine::fold(&ReleaseMark::of_signals(
                &spelled_two_ways,
                &choosing(&["NJ 8255"]),
            )),
            vec![ReleaseMarkLine {
                corroborated: false,
                kind: MarkKind::CatalogNumber,
                value: "NJ-8255".to_string(),
                origins: vec![SignalOrigin::FolderName, SignalOrigin::Artwork],
            }],
        );
    }

    /// A signal the person took out of the run is not a name the disc carries
    /// either: they are saying the code on the box set's sleeve or the ID of
    /// the wrong pressing is not this object's.
    #[test]
    fn a_signal_left_out_of_the_run_is_no_name() {
        assert!(ReleaseMark::of_signals(
            &signals(),
            &LookupChoices {
                disc_id_excluded: true,
                excluded_barcodes: vec!["0075678164521".to_string()],
                ..LookupChoices::default()
            },
        )
        .is_empty());
    }

    /// A folder nothing was read off states no names.
    #[test]
    fn a_folder_that_stated_nothing_marks_nothing() {
        let silent = Signals {
            disc_id: DiscIdSignal::Absent { track_count: 0 },
            verification: None,
            barcode: BarcodeSignal::Absent,
            text: TextSignal::Settled {
                catalogs: Vec::new(),
                free_text: Vec::new(),
            },
            text_pool: Vec::new(),
            durations: crate::import::probe::SourceDurations::default(),
        };
        assert!(ReleaseMark::of_signals(&silent, &LookupChoices::default()).is_empty());
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
