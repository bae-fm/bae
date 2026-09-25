use super::*;

forward! { async this => {
    fn candidate_source_folders(key: String) -> Vec<String> {
        this.services
            .import_candidate_source_folders(&key)
            .await
            .map_err(BridgeError::import)
    }

    /// Read the selected releases as one and answer with the key of the
    /// release they become. The order, the disc layout and the name are
    /// core's; the release's draft is where they are edited afterwards.
    fn combine_candidates(keys: Vec<String>) -> String {
        this.services
            .import_combine_candidates(keys)
            .await
            .map_err(BridgeError::import)
    }

    /// Read every release below the folder `key` names as one — a group
    /// header's "Combine as One Release" — and answer with its key.
    fn combine_folder(key: crate::types::BridgeFolderReleaseDecisionKey) -> String {
        this.services
            .import_combine_folder(key.into_core())
            .await
            .map_err(BridgeError::import)
    }

    /// Read the release at `key` as the folders it is made of.
    fn separate_candidate(key: String) -> () {
        this.services
            .import_separate_candidate(&key)
            .await
            .map_err(BridgeError::import)
    }
} }
