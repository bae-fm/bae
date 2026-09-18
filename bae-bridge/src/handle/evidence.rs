use super::*;
use crate::types::{BridgeEvidenceContent, BridgeEvidenceSelection, BridgeEvidenceSubject};

forward! { async this => {
    fn read_evidence(subject: BridgeEvidenceSubject, selection: BridgeEvidenceSelection) -> Vec<BridgeEvidenceContent> {
        this.services.read_evidence(&subject.into_core(), &selection.into_core())
            .await
            .map(|contents| contents.into_iter().map(BridgeEvidenceContent::from_core).collect())
            .map_err(BridgeError::from)
    }
} }
