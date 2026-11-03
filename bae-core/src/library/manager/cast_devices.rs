//! The receivers the cast picker lists, as the library's `cast_device` table
//! holds them.

use super::*;
use crate::renderer::CastDevice;

impl LibraryManager {
    /// List `devices` as the picker's receivers in place of the last list, in
    /// one write.
    pub async fn show_cast_devices(&self, devices: Vec<CastDevice>) -> Result<(), LibraryError> {
        Ok(crate::renderer::picker::replace_cast_devices(&self.database, devices).await?)
    }

    /// Follow the receivers the picker lists.
    pub(crate) fn subscribe_cast_devices(&self) -> coven::LiveQuery<Vec<CastDevice>> {
        crate::renderer::picker::subscribe_cast_devices(&self.database)
    }
}
