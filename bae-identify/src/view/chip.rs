//! What a chip of the identifier band says: its look and the words after its
//! value, read from the lookups of its ledger row, and the identifier kinds
//! whose chips found releases.

use super::{
    BarcodeStepView, CatalogStepView, DiscIdRow, DiscIdStepView, IdentifyRunView, IsrcStepView,
    LookupView, ProviderCell, SearchStepView, SignalValueRow,
};
use crate::NotAskedReason;
use bae_catalogs::LookupFailure;

/// How a chip looks, from what its lookups did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipLook {
    /// The run leaves the identifier out.
    Off,
    /// The run asks about it.
    On,
    /// A lookup of it found releases.
    Found,
}

/// What a chip says after its value while its lookups are out or failed.
#[derive(Debug, Clone, PartialEq)]
pub enum ChipTrailing {
    Nothing,
    /// A lookup is still out.
    Searching,
    /// A lookup failed, with the first failure.
    Failed {
        failure: LookupFailure,
    },
}

/// A chip: its look and what trails its value.
#[derive(Debug, Clone, PartialEq)]
pub struct ChipState {
    pub look: ChipLook,
    pub trailing: ChipTrailing,
}

/// The identifiers a pressing can be named by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierKind {
    DiscId,
    Barcode,
    Catalog,
}

impl ChipState {
    /// The chip of a value whose lookups are `lookups`: off where the person
    /// left the value out (`left_out`) or a lookup was not asked for that
    /// reason, found where a lookup found releases, on otherwise. Its
    /// trailing words are the first failure, else a lookup still out; a value
    /// left out is asked of nobody, so it has none.
    pub fn of(left_out: bool, lookups: &[&LookupView]) -> Self {
        let left_out_lookup = |lookup: &&LookupView| {
            matches!(lookup, LookupView::NotAsked { reason: NotAskedReason::LeftOut })
        };
        if left_out || lookups.iter().any(left_out_lookup) {
            return Self { look: ChipLook::Off, trailing: ChipTrailing::Nothing };
        }
        let found = lookups
            .iter()
            .any(|lookup| matches!(lookup, LookupView::Found { count, .. } if *count > 0));
        let trailing = lookups
            .iter()
            .find_map(|lookup| match lookup {
                LookupView::Failed { failure } => {
                    Some(ChipTrailing::Failed { failure: failure.clone() })
                }
                _ => None,
            })
            .or_else(|| {
                lookups
                    .iter()
                    .any(|lookup| matches!(lookup, LookupView::LookingUp))
                    .then_some(ChipTrailing::Searching)
            })
            .unwrap_or(ChipTrailing::Nothing);
        Self { look: if found { ChipLook::Found } else { ChipLook::On }, trailing }
    }

    /// The chip of lookups that are one cell per provider.
    pub fn of_cells(left_out: bool, cells: &[ProviderCell]) -> Self {
        let lookups: Vec<&LookupView> = cells.iter().map(|cell| &cell.lookup).collect();
        Self::of(left_out, &lookups)
    }
}

impl DiscIdRow {
    pub fn chip(&self) -> ChipState {
        ChipState::of(false, &[&self.lookup])
    }
}

impl SignalValueRow {
    pub fn chip(&self) -> ChipState {
        ChipState::of_cells(self.excluded, &self.cells)
    }
}

impl IsrcStepView {
    /// The ISRC chip; `None` while the codes are being read or the tags carry
    /// none.
    pub fn chip(&self) -> Option<ChipState> {
        match self {
            Self::Read { lookup, .. } => Some(ChipState::of(false, &[lookup])),
            Self::Reading | Self::Absent => None,
        }
    }
}

impl SearchStepView {
    /// The title search's chip; `None` while no search has gone out.
    pub fn chip(&self) -> Option<ChipState> {
        match self {
            Self::Searched { cells, .. } => Some(ChipState::of_cells(false, cells)),
            Self::NotNeeded | Self::Waiting { .. } => None,
        }
    }
}

impl IdentifyRunView {
    /// The kinds whose chips found releases, in the band's order: they head
    /// the pressing table's columns.
    pub fn found_kinds(&self) -> Vec<IdentifierKind> {
        let found = |chip: ChipState| chip.look == ChipLook::Found;
        let mut kinds = Vec::new();
        if matches!(&self.disc_id, DiscIdStepView::Read { discs } if discs.iter().any(|d| found(d.chip())))
        {
            kinds.push(IdentifierKind::DiscId);
        }
        if matches!(&self.barcode, BarcodeStepView::Rows { rows, .. } if rows.iter().any(|r| found(r.chip())))
        {
            kinds.push(IdentifierKind::Barcode);
        }
        if matches!(&self.catalog, CatalogStepView::Numbers { rows, .. } if rows.iter().any(|r| found(r.chip())))
        {
            kinds.push(IdentifierKind::Catalog);
        }
        kinds
    }
}

#[cfg(test)]
#[path = "chip_tests.rs"]
mod tests;
