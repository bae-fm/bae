//! The import pane's commands: the import service's and the identification
//! queue's own.

use super::*;

impl AppServices {
    delegate_async!(import, pane_start_import => pane_start_import(candidate_key: &bae_import::CandidateKey) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(import, pane_select_release => pane_select_release(candidate_key: bae_import::CandidateKey, link: bae_identify::PressingLink, steps: bae_import::CatalogApplySteps) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(import, pane_read_file_tags => pane_read_file_tags(candidate_key: bae_import::CandidateKey) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(import, pane_unlink_release => pane_unlink_release(candidate_key: &bae_import::CandidateKey) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(import, pane_keep_own_draft => pane_keep_own_draft(candidate_key: &bae_import::CandidateKey) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(import, pane_link_shared_album => pane_link_shared_album(candidate_key: &bae_import::CandidateKey) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(import, pane_start_candidate_search => pane_start_candidate_search(candidate_key: bae_import::CandidateKey, query: bae_catalogs::SearchQuery) -> Result<(), bae_import::ImportError>);
    delegate_async!(identification, pane_edit_lookup_choices => pane_edit_lookup_choices(candidate_key: bae_import::CandidateKey, edit: bae_identify::LookupChoiceEdit) -> Result<bae_import::PaneOutcome, bae_import::ImportError>);
    delegate_async!(identification, pane_identify_automatically => pane_identify_automatically(candidate_key: bae_import::CandidateKey) -> Result<(), bae_import::ImportError>);
    delegate_async!(identification, pane_cancel_identification => pane_cancel_identification(candidate_key: &bae_import::CandidateKey) -> Result<(), bae_import::ImportError>);
}

#[cfg(test)]
#[path = "pane_tests.rs"]
mod tests;
