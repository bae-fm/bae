//! The library's stored Discogs key, read for [`LibraryManager`].

use super::*;

impl LibraryManager {
    pub async fn get_discogs_token(&self) -> Result<Option<String>, LibraryError> {
        self.host_secret(crate::keys::DISCOGS_API_KEY).await
    }
}
