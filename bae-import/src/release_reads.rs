//! Reading a catalog release for import: from the release stored when
//! something last read it, or fetched from its catalog.

use crate::{CatalogApplyStep, CatalogApplySteps, CatalogSessions, ImportError};
use bae_catalogs::payloads::{DiscIdRelease, ReadDepth, ReleasePayloads};
use bae_catalogs::rate_limiter::CallClass;
use bae_catalogs::records::LookupRef;
use bae_catalogs::source_release::PreparedRelease;
use bae_store::Store;

/// Where import reads a catalog release: the store holding releases already
/// read, and the catalogs to fetch one from.
#[derive(Clone)]
pub struct ReleaseReads {
    tables: crate::tables::ImportTables,
    catalogs: CatalogSessions,
}

impl ReleaseReads {
    pub fn new(store: Store, catalogs: CatalogSessions) -> Self {
        Self { tables: crate::tables::ImportTables::new(store), catalogs }
    }

    /// The release `release_ref` names, read to `depth`, from storage or
    /// fetched now. A stored release is fetched again only when that could
    /// add what its fetch missed at this depth: a complete read of a release
    /// an identify run read reads the rest before anything applies it.
    /// Nothing is stored here: the write that names the release stores it.
    pub(crate) async fn release(
        &self,
        release_ref: &LookupRef,
        depth: ReadDepth,
        class: CallClass,
    ) -> Result<PreparedRelease, ImportError> {
        self.prepare(
            release_ref,
            depth,
            self.fetch(release_ref, depth, class),
            &CatalogApplySteps::unread(),
        )
        .await
    }

    /// The release an apply of `release_ref` reads, read whole as
    /// `Self::release` reads it for a person waiting on it, with a fetch
    /// told to `steps`.
    pub async fn applied_release(
        &self,
        release_ref: &LookupRef,
        steps: &CatalogApplySteps,
    ) -> Result<PreparedRelease, ImportError> {
        let depth = ReadDepth::Complete;
        self.prepare(release_ref, depth, self.fetch(release_ref, depth, CallClass::AWAITED), steps)
            .await
    }

    /// The release a Disc ID lookup returned, read as far as an identify run
    /// ranks it: from storage, or read from `answered`, where
    /// [`Self::release`] would fetch it. Only the documents its pages link
    /// are asked for; a complete read of it fetches its own document as well.
    pub(crate) async fn disc_id_release(
        &self,
        answered: &DiscIdRelease,
        class: CallClass,
    ) -> Result<PreparedRelease, ImportError> {
        let read = async {
            Ok(self.catalogs.session().await?.read_disc_id_release(answered, class).await?)
        };
        self.prepare(
            answered.release(),
            ReadDepth::Identification,
            read,
            &CatalogApplySteps::unread(),
        )
        .await
    }

    /// Every partner release of this pick: other catalogs' releases of the
    /// primary's pressing, each fetch told to `steps`. Two releases from one
    /// catalog are refused rather than one chosen.
    pub async fn partners(
        &self,
        primary: &LookupRef,
        partners: &[LookupRef],
        class: CallClass,
        steps: &CatalogApplySteps,
    ) -> Result<Vec<PreparedRelease>, ImportError> {
        let mut claimed = vec![primary.catalog];
        let mut prepared = Vec::with_capacity(partners.len());
        for partner in partners {
            if claimed.contains(&partner.catalog) {
                return Err(ImportError::Internal {
                    detail: format!(
                        "a pick names two {} releases for one pressing",
                        partner.catalog.as_str()
                    ),
                });
            }
            claimed.push(partner.catalog);
            let depth = ReadDepth::Complete;
            prepared.push(
                self.prepare(partner, depth, self.fetch(partner, depth, class), steps).await?,
            );
        }
        Ok(prepared)
    }

    /// `release` fetched from its catalog to `depth`.
    async fn fetch(
        &self,
        release: &LookupRef,
        depth: ReadDepth,
        class: CallClass,
    ) -> Result<ReleasePayloads, ImportError> {
        Ok(self.catalogs.session().await?.fetch_payloads(release, depth, class).await?)
    }

    /// `release_ref` from storage, unless reading it again at `depth` could
    /// add what its stored read missed; otherwise what `read` extracts, told
    /// to `steps` while it reads.
    async fn prepare(
        &self,
        release_ref: &LookupRef,
        depth: ReadDepth,
        read: impl std::future::Future<Output = Result<ReleasePayloads, ImportError>>,
        steps: &CatalogApplySteps,
    ) -> Result<PreparedRelease, ImportError> {
        if let Some(stored) = self.tables.load_prepared_release(release_ref).await? {
            let session = self.catalogs.session().await?;
            if !stored.stored.fetch_could_add(depth, session.asked()) {
                return Ok(stored);
            }
        }
        let payloads = steps
            .during(CatalogApplyStep::ReadingRelease(release_ref.catalog.catalog()), read)
            .await?;
        Ok(self.tables.prepared_release(payloads.extract()?).await?)
    }
}

#[cfg(test)]
#[path = "release_reads_tests.rs"]
mod tests;
