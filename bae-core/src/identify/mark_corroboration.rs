//! A seal belongs to the value whose lookup named the chosen record.

use super::{
    BarcodeStepView, CatalogStepView, DiscIdStepView, IdentifyRunView, LookupProvenance, LookupView,
};
use crate::import::search::MetadataResult;
use crate::import::{MarkKind, MetadataProvenance, MetadataRef, ReleaseMark};

/// Apply the recorded lookup evidence to the folder's readings. Aggregate
/// provenance alone cannot tell which of several barcodes found the record.
pub(crate) fn corroborate_marks<'a>(
    marks: &mut [ReleaseMark],
    picked: Option<&MetadataProvenance>,
    lookups: impl Iterator<Item = (&'a MetadataResult, &'a LookupProvenance)>,
    ledger: Option<&IdentifyRunView>,
) {
    let matched = super::identified_by(picked, lookups).is_some();
    for mark in marks {
        mark.corroborated = match (matched, picked, ledger) {
            (true, Some(MetadataProvenance::ExternalRelease { record, .. }), Some(ledger)) => {
                corroborates(ledger, record, mark.kind, &mark.sighting.value)
            }
            _ => false,
        };
    }
}

fn corroborates(
    ledger: &IdentifyRunView,
    record: &MetadataRef,
    kind: MarkKind,
    value: &str,
) -> bool {
    let rows = match kind {
        MarkKind::DiscId => {
            return matches!(&ledger.disc_id, DiscIdStepView::Read { disc_id, lookup, .. }
                if disc_id == value && names_record(lookup, record));
        }
        MarkKind::Barcode => match &ledger.barcode {
            BarcodeStepView::Rows { rows, .. } => rows,
            _ => return false,
        },
        MarkKind::CatalogNumber => match &ledger.catalog {
            CatalogStepView::Numbers { rows, .. } => rows,
            _ => return false,
        },
    };
    rows.iter().any(|row| {
        kind.same_value(&row.value, value)
            && !row.excluded
            && row
                .cells
                .iter()
                .any(|cell| cell.source == record.catalog && names_record(&cell.lookup, record))
    })
}

fn names_record(lookup: &LookupView, record: &MetadataRef) -> bool {
    let LookupView::Found { groups, .. } = lookup else {
        return false;
    };
    groups
        .iter()
        .flat_map(|group| &group.pressings)
        .flat_map(|pressing| &pressing.releases)
        .any(|release| release.source == record.catalog && release.release_id == record.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identify::{ProviderCell, SignalValueRow};
    use crate::import::release_group::{group_results, unranked};
    use crate::import::Catalog;
    use crate::signals::{SignalOrigin, SourcedValue};

    fn mark(kind: MarkKind, value: &str) -> ReleaseMark {
        ReleaseMark {
            kind,
            sighting: SourcedValue::new(value.to_string(), SignalOrigin::Artwork),
            corroborated: false,
        }
    }

    #[test]
    fn a_seal_requires_the_value_and_the_chosen_catalog_record_to_match() {
        let result = MetadataResult::for_test(Catalog::MusicBrainz, "rel-1", None);
        let lookup = LookupView::Found {
            count: 1,
            groups: group_results(unranked(vec![result.clone()])),
        };
        let row = |value: &str| SignalValueRow {
            value: value.to_string(),
            sources: Vec::new(),
            excluded: false,
            cells: vec![ProviderCell {
                source: result.source,
                lookup: lookup.clone(),
            }],
        };
        let ledger = IdentifyRunView {
            providers: vec![result.source],
            disc_id: DiscIdStepView::Read {
                disc_id: "aBc-1".into(),
                source: None,
                lookup: lookup.clone(),
            },
            barcode: BarcodeStepView::Rows {
                scanning: false,
                rows: vec![row("1234567890123")],
            },
            catalog: CatalogStepView::Numbers {
                scanning: false,
                rows: vec![row("AB-123")],
                candidates: Vec::new(),
            },
        };
        let provenance = LookupProvenance {
            by_disc_id: true,
            by_barcode: true,
            by_catalog: true,
        };
        let mut marks = vec![
            mark(MarkKind::DiscId, "aBc-1"),
            mark(MarkKind::DiscId, "abc-1"),
            mark(MarkKind::Barcode, "1234567890123"),
            mark(MarkKind::Barcode, "1234567890999"),
            mark(MarkKind::CatalogNumber, "ab 123"),
            mark(MarkKind::CatalogNumber, "AB-456"),
        ];
        for (picked, expected) in [
            (
                Some(MetadataProvenance::ExternalRelease {
                    record: MetadataRef::new(result.source, "rel-1"),
                    partners: Vec::new(),
                }),
                vec![true, false, true, false, true, false],
            ),
            (
                Some(MetadataProvenance::ExternalRelease {
                    record: MetadataRef::new(Catalog::Discogs, "rel-1"),
                    partners: Vec::new(),
                }),
                vec![false; 6],
            ),
            (
                Some(MetadataProvenance::ExternalRelease {
                    record: MetadataRef::new(result.source, "rel-other"),
                    partners: Vec::new(),
                }),
                vec![false; 6],
            ),
            (Some(MetadataProvenance::FileTags), vec![false; 6]),
            (None, vec![false; 6]),
        ] {
            corroborate_marks(
                &mut marks,
                picked.as_ref(),
                std::iter::once((&result, &provenance)),
                Some(&ledger),
            );
            assert_eq!(
                marks
                    .iter()
                    .map(|mark| mark.corroborated)
                    .collect::<Vec<_>>(),
                expected
            );
        }
        let picked = MetadataProvenance::ExternalRelease {
            record: MetadataRef::new(result.source, "rel-1"),
            partners: Vec::new(),
        };
        corroborate_marks(&mut marks, Some(&picked), std::iter::empty(), Some(&ledger));
        assert!(
            marks.iter().all(|mark| !mark.corroborated),
            "partial evidence from an unsuccessful verdict is not a match"
        );
        corroborate_marks(
            &mut marks,
            Some(&picked),
            std::iter::once((&result, &provenance)),
            None,
        );
        assert!(
            marks.iter().all(|mark| !mark.corroborated),
            "aggregate provenance cannot prove which value matched"
        );
    }
}
