//! This device's open invites, as People shows them, and setting a
//! person's avatar from a picked image.

use bae_db::people::ProfileError;

/// One of this device's open invites, with the pending record of its
/// operation when coven holds one: a refused check or a failed grant, which
/// the person ends by cancelling the invite (§5.6).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenInvite {
    pub invite: coven::Invite,
    pub held: Option<bae_db::PendingWait>,
}

impl OpenInvite {
    /// Each of `invites` with the record among `records` whose subject is
    /// its operation.
    pub(crate) fn held_by(
        invites: Vec<coven::Invite>,
        records: &[coven::PendingRecord],
    ) -> Vec<Self> {
        invites
            .into_iter()
            .map(|invite| {
                let held = records
                    .iter()
                    .find(|record| {
                        matches!(record.subject,
                            coven::PendingSubject::Operation { id, .. } if id == invite.operation)
                    })
                    .map(|record| bae_db::PendingWait::of(record, None));
                Self { invite, held }
            })
            .collect()
    }
}

/// Where an open invite stands, as People shows it (§5.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InviteShown {
    /// Sync is granting the invite's access; its code comes once it has.
    Granting,
    /// Its code, to show again, and how far its person has come.
    Code { code: String, progress: InviteProgress },
}

/// How far an invite whose code can be shown has come.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InviteProgress {
    /// Waiting for its person's request.
    Waiting,
    /// A sent invite's request, waiting for the admin, who approves it only
    /// after comparing the confirmation number. An in-person invite's
    /// request is approved without the admin.
    Deciding(coven::JoinRequest),
    /// Its person is being let in, which finishes without the admin; the
    /// invite can no longer be cancelled.
    Joining,
}

/// Where `invite` stands.
pub fn invite_shown(invite: &coven::Invite) -> InviteShown {
    match &invite.stage {
        coven::InviteStage::Granting => InviteShown::Granting,
        coven::InviteStage::Waiting { code, request } => InviteShown::Code {
            code: code.clone(),
            progress: match (request, invite.delivery) {
                (Some(request), coven::InviteDeliveryKind::Sent) => {
                    InviteProgress::Deciding(request.clone())
                }
                (Some(_), coven::InviteDeliveryKind::InPerson) | (None, _) => {
                    InviteProgress::Waiting
                }
            },
        },
        coven::InviteStage::Approving { code, .. } => {
            InviteShown::Code { code: code.clone(), progress: InviteProgress::Joining }
        }
    }
}

/// Setting this person's avatar failed: the picked image, or the
/// profile's write.
#[derive(Debug, thiserror::Error)]
pub enum AvatarError {
    /// The image picked for an avatar cannot be decoded or resized.
    #[error("the avatar image failed: {detail}")]
    Image { detail: String },
    /// Resizing the avatar ran apart from its caller and gave no result: it
    /// panicked, which the caller reports as a bug, or its runtime is shutting
    /// down.
    #[error(transparent)]
    Unfinished(#[from] bae_util::task_failure::TaskFailed),
    #[error(transparent)]
    Profile(#[from] ProfileError),
}

impl AvatarError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        use bae_diagnostics::UiErrorCategory as C;
        match self {
            Self::Image { .. } => C::Config,
            Self::Unfinished(_) => C::Internal,
            Self::Profile(error) => error.category(),
        }
    }
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;
