use super::*;

#[uniffi::export]
impl AppHandle {
    pub fn subscribe_album_parent_observation(
        &self,
        callback: Box<dyn crate::types::LibraryParentObservationCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let query = self.services.subscribe_album_parent_observation();
        subscribe_library_parent_observation(self.runtime.handle(), query, callback)
    }

    pub fn subscribe_composer_parent_observation(
        &self,
        callback: Box<dyn crate::types::LibraryParentObservationCallback>,
    ) -> std::sync::Arc<crate::LiveSubscription> {
        let query = self.services.subscribe_composer_parent_observation();
        subscribe_library_parent_observation(self.runtime.handle(), query, callback)
    }
}

fn subscribe_library_parent_observation(
    runtime: &tokio::runtime::Handle,
    mut query: coven::LiveQuery<bae_core::db::LibraryParentObservationProjection>,
    callback: Box<dyn crate::types::LibraryParentObservationCallback>,
) -> std::sync::Arc<crate::LiveSubscription> {
    let task = runtime.spawn(async move {
        loop {
            match query.next().await {
                Ok(value) => callback.on_value(crate::types::BridgeLibraryParentObservation {
                    child_count: value.child_count,
                }),
                Err(error) => callback.on_error(BridgeError::database(error)),
            }
        }
    });
    std::sync::Arc::new(crate::LiveSubscription::new(task))
}
