use super::detail::live_read_error;
use super::*;

mirror_enum! {
    crate::types::BridgeReleaseFileColumn = bae_core::library::ReleaseFileColumn,
    from_core: fn,
    into_core: fn,
    variants: { Name, Size, Kind },
}

impl crate::types::BridgeReleaseFilesRequest {
    fn into_core(self) -> bae_core::library::ReleaseFilesRequest {
        bae_core::library::ReleaseFilesRequest {
            release_id: self.release_id,
            order: bae_core::library::ReleaseFileOrder {
                column: self.order.column.into_core(),
                descending: self.order.descending,
            },
        }
    }

    fn from_core(request: bae_core::library::ReleaseFilesRequest) -> Self {
        Self {
            release_id: request.release_id,
            order: crate::types::BridgeReleaseFileOrder {
                column: crate::types::BridgeReleaseFileColumn::from_core(request.order.column),
                descending: request.order.descending,
            },
        }
    }
}

/// A release's files in the order the Storage sheet's file table sorts by,
/// read live: one read for as long as the sheet shows, moved in place as its
/// order changes.
#[derive(uniffi::Object)]
pub struct ReleaseFilesSubscription {
    inner: bae_core::library::ReleaseFilesSubscription,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    /// A read that shows nothing until `set_request` names a release.
    pub fn subscribe_release_files(&self) -> std::sync::Arc<ReleaseFilesSubscription> {
        std::sync::Arc::new(ReleaseFilesSubscription {
            inner: self.services.subscribe_release_files(),
            runtime: self.runtime.clone(),
        })
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl ReleaseFilesSubscription {
    /// Read `request` from now on; `None` reads nothing.
    pub fn set_request(
        &self,
        request: Option<crate::types::BridgeReleaseFilesRequest>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(request.map(crate::types::BridgeReleaseFilesRequest::into_core))
            .map_err(live_read_error)
    }

    pub async fn next(
        self: std::sync::Arc<Self>,
    ) -> Result<crate::types::BridgeReleaseFilesSnapshot, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner
                .next()
                .await
                .map(|snapshot| crate::types::BridgeReleaseFilesSnapshot {
                    request: snapshot
                        .request
                        .map(crate::types::BridgeReleaseFilesRequest::from_core),
                    files: snapshot
                        .files
                        .map(|files| files.into_iter().map(BridgeFile::from_core).collect()),
                })
                .map_err(live_read_error)
        })
        .await
    }

    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            self.inner.cancel();
            Ok(())
        })
        .await
    }
}
