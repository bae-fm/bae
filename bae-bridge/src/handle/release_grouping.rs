use super::*;

forward! { async this => {
    /// The folders the release at `key` is read from, as coven's bytes for
    /// each path.
    fn candidate_source_folders(key: Vec<u8>) -> Vec<Vec<u8>> {
        Ok(this
            .services
            .import_candidate_source_folders(&key.into())
            .await
            .map_err(BridgeError::import)?
            .iter()
            .map(|folder| coven::path_bytes(folder))
            .collect())
    }

    /// Read every release below the folder `key` names as one — a group
    /// header's "Combine as One Release" — and answer with its key.
    fn combine_folder(key: crate::types::BridgeFolderReleaseDecisionKey) -> Vec<u8> {
        Ok(this
            .services
            .import_combine_folder(key.into_core()?)
            .await
            .map_err(BridgeError::import)?
            .into_bytes())
    }

    /// Read the release at `key` as the folders it is made of — a broken
    /// combined row's "Keep as Separate Releases", which selects nothing.
    fn separate_candidate(key: Vec<u8>) -> () {
        this.services
            .import_separate_candidate(&key.into())
            .await
            .map(|_| ())
            .map_err(BridgeError::import)
    }
} }
