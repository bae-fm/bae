//! Uploads this device abandoned because their rows took a newer change first,
//! as the sync activity lists them. Mirrors `bae_core::library::AbandonedItem`.

use super::{BridgeError, BridgeMessage};

/// coven's record of one abandoned attachment, opaque to the UI, which hands
/// it back to `dismiss_abandoned_attachment`.
#[derive(Debug, uniffi::Object)]
pub struct BridgeAbandonedRecord(pub(crate) coven::AbandonedAttachment);

/// One upload that will never land: what it was and why, with the record
/// dismissing it forgets.
#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeAbandonedAttachment {
    /// Stable across values while the record is listed.
    pub id: String,
    /// What the file was, by its name and album where its row still says.
    pub subject: BridgeMessage,
    /// Why the upload was abandoned, as a catalog key.
    pub reason_key: String,
    pub record: std::sync::Arc<BridgeAbandonedRecord>,
}

impl BridgeAbandonedAttachment {
    fn from_core(item: bae_core::library::AbandonedItem) -> Self {
        Self {
            id: item.record.file.0.to_string(),
            subject: super::subject_message(item.subject),
            reason_key: bridge_abandon_reason_key(item.record.reason).to_string(),
            record: std::sync::Arc::new(BridgeAbandonedRecord(item.record)),
        }
    }
}

/// The abandoned uploads, as the activity list reads them.
#[derive(Debug, Clone, uniffi::Enum)]
pub enum BridgeAbandonedAttachments {
    Loading,
    Ready {
        attachments: Vec<BridgeAbandonedAttachment>,
    },
    Failed {
        error: BridgeError,
    },
}

impl BridgeAbandonedAttachments {
    pub(crate) fn from_core(
        value: Option<Result<Vec<bae_core::library::AbandonedItem>, bae_core::ui::UiError>>,
    ) -> Self {
        match value {
            None => Self::Loading,
            Some(Ok(items)) => Self::Ready {
                attachments: items
                    .into_iter()
                    .map(BridgeAbandonedAttachment::from_core)
                    .collect(),
            },
            Some(Err(error)) => Self::Failed {
                error: BridgeError::from_core(error),
            },
        }
    }
}

/// The catalog key saying why an upload was abandoned.
pub(crate) fn bridge_abandon_reason_key(reason: coven::AbandonReason) -> &'static str {
    match reason {
        coven::AbandonReason::Replaced => "core.abandoned.reason.replaced",
        coven::AbandonReason::Cleared => "core.abandoned.reason.cleared",
        coven::AbandonReason::RowGone => "core.abandoned.reason.row_gone",
    }
}
