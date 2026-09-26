//! How identification runs, as `preferences.yaml`'s `identification` mapping
//! carries it: whether it starts on its own, which steps every run takes, and
//! which catalogs it asks.

use serde::{Deserialize, Serialize};

/// Everything a person decides about identification, in one place. Every
/// setting defaults to what identification did before it was a setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentificationPreferences {
    /// Whether identification starts on its own: the automatic admission
    /// queues every candidate that has no result for its current files.
    /// Defaults to `true`; off means no new candidate is queued on its own —
    /// what is already queued finishes, and a person starts each run
    /// themselves.
    pub automatic: bool,
    /// The steps every run takes, each of which can be switched off.
    pub steps: IdentificationSteps,
    /// Which catalogs identification asks — the automatic run, the typed
    /// search, and every retry.
    pub catalogs: LookupCatalogPreferences,
}

impl Default for IdentificationPreferences {
    fn default() -> Self {
        Self {
            automatic: true,
            steps: IdentificationSteps::default(),
            catalogs: LookupCatalogPreferences::default(),
        }
    }
}

/// One step of an identification run a person can switch off. What each step
/// is, and what switching it off leaves the run with, is here; where each is
/// read is the step itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdentificationStep {
    /// Read the folder's cover art: detect the barcodes on it and recognize
    /// its text. Off, no image is read, and the barcodes and catalog numbers
    /// the art would have given are not read rather than not found; the
    /// folder's other text and its CUE sheets are still read.
    ReadCoverArt,
    /// Ask MusicBrainz about the disc ID a rip log or CUE sheet gives. Off,
    /// the disc ID is still read and shown, and nobody is asked about it.
    LookUpDiscIds,
    /// Ask every catalog about the folder's barcodes. Off, the codes are still
    /// read and shown, and nobody is asked about them.
    LookUpBarcodes,
    /// Search every catalog by the release's title when its codes name
    /// nothing. Off, a folder whose codes name nothing is left unmatched.
    SearchByTitle,
    /// Read what the MusicBrainz albums a run found are on Discogs — the
    /// album's own page, its Wikidata item, and the releases it links — so
    /// the two catalogs' records of one album go on one card. Off, each
    /// catalog's records stand on their own cards unless a barcode already
    /// ties them.
    FollowCatalogLinks,
}

impl IdentificationStep {
    /// Every step, in the order a run takes them — the order a surface lists
    /// their switches in.
    pub const ALL: [IdentificationStep; 5] = [
        IdentificationStep::ReadCoverArt,
        IdentificationStep::LookUpDiscIds,
        IdentificationStep::LookUpBarcodes,
        IdentificationStep::SearchByTitle,
        IdentificationStep::FollowCatalogLinks,
    ];
}

/// Which of a run's steps it takes. One flag per [`IdentificationStep`], all
/// on by default.
///
/// The YAML mapping needs a name per step, so the fields are named, and each
/// step reads its own flag where it runs. Nothing that works over the steps
/// as a set reads them by name: [`Self::takes`] and [`Self::set`] are total
/// over [`IdentificationStep`], so adding one fails the build here rather than
/// silently defaulting.
///
/// A run reads these once, as it starts, beside its providers: a change
/// applies to the next run, and a run in flight finishes the way it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdentificationSteps {
    pub read_cover_art: bool,
    pub look_up_disc_ids: bool,
    pub look_up_barcodes: bool,
    pub search_by_title: bool,
    pub follow_catalog_links: bool,
}

impl IdentificationSteps {
    pub fn takes(&self, step: IdentificationStep) -> bool {
        match step {
            IdentificationStep::ReadCoverArt => self.read_cover_art,
            IdentificationStep::LookUpDiscIds => self.look_up_disc_ids,
            IdentificationStep::LookUpBarcodes => self.look_up_barcodes,
            IdentificationStep::SearchByTitle => self.search_by_title,
            IdentificationStep::FollowCatalogLinks => self.follow_catalog_links,
        }
    }

    pub fn set(&mut self, step: IdentificationStep, enabled: bool) {
        let flag = match step {
            IdentificationStep::ReadCoverArt => &mut self.read_cover_art,
            IdentificationStep::LookUpDiscIds => &mut self.look_up_disc_ids,
            IdentificationStep::LookUpBarcodes => &mut self.look_up_barcodes,
            IdentificationStep::SearchByTitle => &mut self.search_by_title,
            IdentificationStep::FollowCatalogLinks => &mut self.follow_catalog_links,
        };
        *flag = enabled;
    }
}

impl Default for IdentificationSteps {
    fn default() -> Self {
        Self {
            read_cover_art: true,
            look_up_disc_ids: true,
            look_up_barcodes: true,
            search_by_title: true,
            follow_catalog_links: true,
        }
    }
}

/// Which catalogs this library asks. One flag per
/// [`Catalog::LOOKUP`](crate::import::Catalog::LOOKUP) member, all on by
/// default.
///
/// The YAML mapping needs a name per catalog, so the fields are named — but
/// nothing reads them by name: [`Self::enabled`] and [`Self::set`] are total
/// over the asked catalogs, so adding one fails the build here rather than
/// silently defaulting. Neither is the main one; a catalog switched off is not
/// asked by anything that asks the catalogs together.
///
/// A flag stays as the person set it while the source is unreachable for
/// another reason (Discogs without a key), so supplying the key restores their
/// choice rather than turning the source on behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupCatalogPreferences {
    pub musicbrainz: bool,
    pub discogs: bool,
}

impl LookupCatalogPreferences {
    pub fn enabled(&self, catalog: crate::import::Catalog) -> bool {
        match catalog {
            crate::import::Catalog::MusicBrainz => self.musicbrainz,
            crate::import::Catalog::Discogs => self.discogs,
            other => unreachable!("{} answers no lookups", other.as_str()),
        }
    }

    pub fn set(&mut self, catalog: crate::import::Catalog, enabled: bool) {
        match catalog {
            crate::import::Catalog::MusicBrainz => self.musicbrainz = enabled,
            crate::import::Catalog::Discogs => self.discogs = enabled,
            other => unreachable!("{} answers no lookups", other.as_str()),
        }
    }
}

impl Default for LookupCatalogPreferences {
    fn default() -> Self {
        Self {
            musicbrainz: true,
            discogs: true,
        }
    }
}
