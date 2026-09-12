use super::*;

forward! { async this => {
    fn candidate_source_folders(key: String) -> Vec<String> {
        this.services
            .import_candidate_source_folders(&key)
            .await
            .map_err(BridgeError::import)
    }

    /// Make the selected folders one release and answer with its candidate key.
    /// The order, the disc layout and the name are core's; the combined
    /// candidate's draft is where they are edited afterwards.
    fn combine_candidates(keys: Vec<String>) -> String {
        this.services
            .import_combine_candidates(keys)
            .await
            .map_err(BridgeError::import)
    }

    fn separate_combined_candidate(key: String) -> () {
        this.services
            .import_separate_combined_candidate(&key)
            .await
            .map_err(BridgeError::import)
    }
} }
