//! Facts and actions for the selected import rows, independent of album editors.

use super::triage::{candidate_actions, import_status_of, place, CandidateAction, CandidateAnswer};
use super::{ImportedRelease, MetadataProvenance, TriageRuntimeFacts};
use crate::identify::QueueClassification;
use std::collections::BTreeMap;

/// An action retains its key and display label across selection changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidateActionTarget {
    pub key: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCandidateActionOffer {
    pub action: CandidateAction,
    pub candidates: Vec<ImportCandidateActionTarget>,
}

/// The complete bulk pane value. Ordering and eligibility belong to core.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImportSelection {
    pub candidate_keys: Vec<String>,
    pub offers: Vec<ImportCandidateActionOffer>,
    pub can_combine: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SelectedCandidateFacts {
    pub target: ImportCandidateActionTarget,
    pub actionable: bool,
    pub is_folder: bool,
    pub skipped: bool,
    pub imported: Option<ImportedRelease>,
    pub failure: Option<String>,
    pub provenance: Option<MetadataProvenance>,
    pub metadata_valid: bool,
    pub answer: Option<QueueClassification>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportSelectionProjection(pub Vec<SelectedCandidateFacts>);

impl ImportSelectionProjection {
    pub(crate) fn resolve(
        &self,
        runtime: &BTreeMap<String, TriageRuntimeFacts>,
    ) -> ImportSelection {
        let mut value = ImportSelection::default();
        let mut actions = Vec::new();
        let mut combinable = 0;
        for candidate in &self.0 {
            let idle = TriageRuntimeFacts::default();
            let facts = runtime.get(&candidate.target.key).unwrap_or(&idle);
            let status = import_status_of(
                facts.importing,
                candidate.imported.as_ref(),
                candidate.failure.as_deref(),
            );
            let answer = match (
                candidate.answer.as_ref().filter(|_| candidate.actionable),
                &facts.identification,
            ) {
                (Some(answer), _) => CandidateAnswer::Classified(answer.clone()),
                (None, Some(status)) => CandidateAnswer::Identification(status.clone()),
                (None, None) => CandidateAnswer::Unidentified,
            };
            let placement = place(
                candidate.skipped,
                candidate.imported.is_some(),
                status.as_ref(),
                candidate
                    .provenance
                    .as_ref()
                    .filter(|_| candidate.actionable),
                candidate.metadata_valid,
                &answer,
            );
            actions.push(candidate_actions(
                candidate.actionable,
                &placement,
                facts.identification.as_ref(),
                &answer,
            ));
            if candidate.is_folder
                && super::combination::CombinationAction::Combine
                    .available(candidate.actionable, candidate.imported.is_some(), facts)
                    .is_some()
            {
                combinable += 1;
            }
            value.candidate_keys.push(candidate.target.key.clone());
        }
        value.can_combine = self.0.len() >= 2 && combinable == self.0.len();
        for action in [
            CandidateAction::ImportReady,
            CandidateAction::Identify,
            CandidateAction::RetryIdentification,
            CandidateAction::UseFileMetadata,
            CandidateAction::ClearMetadata,
            CandidateAction::Skip,
            CandidateAction::Restore,
        ] {
            let candidates: Vec<_> = self
                .0
                .iter()
                .zip(&actions)
                .filter(|(_, available)| available.contains(&action))
                .map(|(candidate, _)| candidate.target.clone())
                .collect();
            if !candidates.is_empty() {
                value
                    .offers
                    .push(ImportCandidateActionOffer { action, candidates });
            }
        }
        value
    }
}
