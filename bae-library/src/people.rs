//! Where an open invite stands, as People shows it, and setting a person's
//! avatar from a picked image.

use bae_people::people::ProfileError;

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
    /// Its person's request, waiting for the admin, who approves it only
    /// after comparing the confirmation number.
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
            progress: match request {
                Some(request) => InviteProgress::Deciding(request.clone()),
                None => InviteProgress::Waiting,
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
