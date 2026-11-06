//! A release's files as the Storage sheet's file table pulls them.

use crate::types::{BridgeError, BridgeFile};

mirror_enum! {
    crate::types::BridgeReleaseFileColumn = bae_views::ReleaseFileColumn,
    from_core: fn,
    into_core: fn,
    variants: { Name, Size, Kind },
}

impl crate::types::BridgeReleaseFilesRequest {
    fn into_core(self) -> bae_views::ReleaseFilesRequest {
        bae_views::ReleaseFilesRequest {
            release_id: self.release_id,
            order: bae_views::ReleaseFileOrder {
                column: self.order.column.into_core(),
                descending: self.order.descending,
            },
        }
    }

    fn from_core(request: bae_views::ReleaseFilesRequest) -> Self {
        Self {
            release_id: request.release_id,
            order: crate::types::BridgeReleaseFileOrder {
                column: crate::types::BridgeReleaseFileColumn::from_core(request.order.column),
                descending: request.order.descending,
            },
        }
    }
}

impl crate::types::BridgeReleaseFilesSnapshot {
    fn from_core(snapshot: bae_views::ReleaseFilesSnapshot) -> Self {
        Self {
            request: snapshot.request.map(crate::types::BridgeReleaseFilesRequest::from_core),
            files: snapshot
                .files
                .map(|files| files.into_iter().map(BridgeFile::from_core).collect()),
        }
    }
}

pulled_read! {
    /// A release's files in the order the Storage sheet's file table sorts
    /// by, read live: one read for as long as the sheet shows, moved in place
    /// as its order changes. It shows nothing until `set_request` names a
    /// release.
    ReleaseFilesSubscription:
        bae_app::live_reads::LiveReleaseFiles => crate::types::BridgeReleaseFilesSnapshot,
    converted by crate::types::BridgeReleaseFilesSnapshot::from_core,
    built by release_files
}

#[uniffi::export]
impl ReleaseFilesSubscription {
    /// Read `request` from now on; `None` reads nothing.
    pub fn set_request(
        &self,
        request: Option<crate::types::BridgeReleaseFilesRequest>,
    ) -> Result<(), BridgeError> {
        self.inner
            .set(request.map(crate::types::BridgeReleaseFilesRequest::into_core))
            .map_err(BridgeError::from)
    }
}
