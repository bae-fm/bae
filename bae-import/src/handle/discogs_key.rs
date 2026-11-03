//! The library's Discogs key: checked against Discogs when the person
//! submits it, stored with what the check said, checked again on request,
//! shown and removed.

use super::*;
use bae_catalogs::providers::DiscogsCheck;
use bae_catalogs::rate_limiter::CallPriority;

/// What `save_discogs_token` did with a submitted key after checking it with
/// Discogs.
///
/// - `Valid` — Discogs accepted the key; it is stored.
/// - `Unvalidated` — Discogs could not be asked (unreachable, rate-limited, an
///   answer bae could not read); the key is stored and re-checked later, and
///   the failure says why.
/// - `Rejected` — Discogs returned 401; nothing is stored.
#[derive(Debug)]
pub enum DiscogsSaveOutcome {
    Valid,
    Unvalidated(bae_catalogs::discogs::client::DiscogsError),
    Rejected,
}

impl ImportServiceHandle {
    /// Check a submitted Discogs key against Discogs and store it unless
    /// Discogs rejects it; a key that could not be checked is stored as
    /// unvalidated.
    pub async fn save_discogs_token(
        &self,
        token: &str,
    ) -> Result<DiscogsSaveOutcome, crate::ImportError> {
        let check =
            DiscogsCheck::of(self.catalogs.try_discogs_key(token, CallPriority::Interactive).await);
        let validation = bae_preferences::DiscogsValidation::of(&check);
        match check {
            DiscogsCheck::Valid => {
                self.persist_discogs_key(token, validation).await?;
                Ok(DiscogsSaveOutcome::Valid)
            }
            DiscogsCheck::Unchecked(error) => {
                self.persist_discogs_key(token, validation).await?;
                Ok(DiscogsSaveOutcome::Unvalidated(error))
            }
            DiscogsCheck::Rejected => Ok(DiscogsSaveOutcome::Rejected),
        }
    }

    /// Store the key and record its validation.
    async fn persist_discogs_key(
        &self,
        token: &str,
        validation: bae_preferences::DiscogsValidation,
    ) -> Result<(), crate::ImportError> {
        Ok(self.catalogs.store_discogs_key(token, validation).await?)
    }

    /// Check a stored `Unvalidated` key against Discogs again; any other
    /// state, or no key, is left alone. A check Discogs could not answer
    /// leaves the key unvalidated and fails with why, so the person who asked
    /// is told rather than shown the same state again.
    pub async fn revalidate_discogs_token(&self) -> Result<(), crate::ImportError> {
        use bae_preferences::DiscogsValidation;

        if bae_preferences::preferences(&self.database).await?.discogs
            != Some(DiscogsValidation::Unvalidated)
        {
            return Ok(());
        }
        let check = self.catalogs.session().await?.validate().await.ok_or_else(|| {
            crate::ImportError::Internal {
                detail: "config says a Discogs key is stored but the keyring has none".to_string(),
            }
        })?;
        let validation = DiscogsValidation::of(&check);
        bae_preferences::edit_preferences(&self.database, move |prefs| {
            if prefs.discogs.is_some() {
                prefs.discogs = Some(validation);
            }
            Ok(())
        })
        .await
        .map_err(crate::DiscogsKeyError::from)?;
        match check {
            DiscogsCheck::Valid | DiscogsCheck::Rejected => Ok(()),
            DiscogsCheck::Unchecked(error) => Err(bae_catalogs::CatalogError::from(error).into()),
        }
    }

    /// The library's stored Discogs key, as the settings screen shows it.
    pub async fn discogs_token(&self) -> Result<Option<String>, crate::ImportError> {
        Ok(self.catalogs.stored_discogs_key().await?)
    }

    /// Remove the stored Discogs key and its validation.
    pub async fn remove_discogs_token(&self) -> Result<(), crate::ImportError> {
        Ok(self.catalogs.clear_discogs_key().await?)
    }
}
