//! The library's Discogs key as the import service offers it: checked
//! against Discogs and stored when the person submits it, checked again on
//! request, shown and removed. [`crate::CatalogSessions`] owns the key and
//! what Discogs said of it.

use super::*;
use bae_catalogs::providers::DiscogsCheck;

impl ImportServiceHandle {
    /// Check a submitted Discogs key against Discogs and store it unless
    /// Discogs rejects it; a key checked while offline is stored as
    /// unvalidated. Discogs answering with no verdict stores nothing and
    /// fails.
    pub async fn save_discogs_token(
        &self,
        token: &str,
    ) -> Result<DiscogsCheck, crate::ImportError> {
        Ok(self.services.catalogs.save_discogs_key(token).await?)
    }

    /// Check a stored `Unvalidated` key against Discogs again; any other
    /// state, or no key, is left alone. Still offline leaves the key
    /// unvalidated, which the key's state already says; Discogs answering
    /// with no verdict leaves it so and fails, so the person who asked is
    /// told.
    pub async fn revalidate_discogs_token(&self) -> Result<(), crate::ImportError> {
        self.services.catalogs.revalidate_discogs_key().await?;
        Ok(())
    }

    /// The library's stored Discogs key, as the settings screen shows it.
    pub async fn discogs_token(&self) -> Result<Option<String>, crate::ImportError> {
        Ok(self.services.catalogs.stored_discogs_key().await?)
    }

    /// Remove the stored Discogs key and its validation.
    pub async fn remove_discogs_token(&self) -> Result<(), crate::ImportError> {
        Ok(self.services.catalogs.clear_discogs_key().await?)
    }
}
