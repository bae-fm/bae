//! Making this device's copy of a library: creating a library, restoring
//! one from a restore code, joining one from an invite, and resetting or
//! replacing this device's copy.
//!
//! Each function here awaits coven's calls on this device's installation, so
//! it reports a bug one carries back to the `diagnostics` it is handed, as
//! the store reports its own calls'.

mod auth;

use bae_store::configure_store;
use tokio::sync::watch;

#[derive(Debug, thiserror::Error)]
pub enum RestoreFromCodeError {
    #[error(transparent)]
    Profile(#[from] bae_people::people::ProfileError),
    #[error("restore cancelled")]
    Cancelled,
    #[error("restore failed: {0}")]
    Restore(#[source] Box<coven::BootstrapError>),
    /// This device's copy couldn't be read for the settings a restore over
    /// it keeps, so nothing was restored.
    #[error("this device's copy couldn't be read for its settings: {0}")]
    SettingsUnread(#[source] Box<crate::LibraryError>),
}

impl RestoreFromCodeError {
    /// What the person can do about the failure: fix the code, the cloud
    /// credentials, or the keyring, or retry once the cloud is reachable.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Profile(error) => error.category(),
            Self::Cancelled => bae_diagnostics::UiErrorCategory::Internal,
            Self::Restore(error) => bae_diagnostics::bootstrap_category(error),
            Self::SettingsUnread(error) => error.category(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateLibraryError {
    #[error(transparent)]
    Store(#[from] coven::StoreCreationError),
}

impl CreateLibraryError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Store(error) => bae_diagnostics::creation_category(error),
        }
    }
}

/// `builder` with its file transfers limited as `limits` says, per direction.
pub(crate) fn with_transfer_limits(
    builder: coven::CovenBuilder,
    limits: coven::TransferLimits,
) -> coven::CovenBuilder {
    builder.max_concurrent_uploads(limits.uploads).max_concurrent_downloads(limits.downloads)
}

/// `builder` configured as bae's store for loading a library from storage,
/// under the default transfer limits: the library's own preferences are in
/// the store being loaded, so they apply once it opens.
fn loading_builder(builder: coven::CovenBuilder) -> coven::CovenBuilder {
    with_transfer_limits(
        configure_store(builder, crate::SCHEMA),
        bae_app_dir::default_transfer_limits(),
    )
}

/// Create a store and retain its open handle, including its identity custody.
pub async fn create_library(
    builder: coven::CovenBuilder,
    name: bae_store::EnteredName,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, CreateLibraryError> {
    configure_store(builder, crate::SCHEMA)
        .create_store(name.as_str())
        .await
        .map_err(|error| CreateLibraryError::Store(coven::task::reported(diagnostics, error)))
}

/// Restore with a separately entered S3 key and coven's own OAuth presentation.
/// The returned handle retains session custody; the caller keeps it for the
/// running library instead of reopening from a second configuration copy.
pub async fn restore_from_code(
    mut builder: coven::CovenBuilder,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    let metadata = auth::code_metadata(code)?;
    auth::authenticate(&mut builder, metadata.oauth_access, cancel, diagnostics).await?;
    coven::restore_from_code(
        loading_builder(builder),
        code,
        s3_credentials,
        device_name,
        on_status,
        cancel,
    )
    .await
    .map_err(|error| reported_restore_error(diagnostics, error))
}

pub async fn restore_from_saved_code(
    mut builder: coven::CovenBuilder,
    saved: &coven::SavedRestoreCode,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<coven::CovenHandle>, RestoreFromCodeError> {
    auth::authenticate(&mut builder, saved.location.oauth_access(), cancel, diagnostics).await?;
    coven::restore_from_saved_code(
        loading_builder(builder),
        saved,
        s3_credentials,
        device_name,
        on_status,
        cancel,
    )
    .await
    .map_err(|error| reported_restore_error(diagnostics, error))
}

/// The membership is established even if asking for its profile failed.
/// Retain this handle and retry profile creation; do not join as another
/// identity. The profile request is `None` for an invite that replaced an
/// existing member's keys: that person keeps the row they already have
/// (§5.4). An asked-for profile is written once sync has made the person's
/// private circle.
pub type JoinedLibrary = (coven::CovenHandle, Option<Result<(), bae_people::people::ProfileError>>);

/// The app's hold on a running join: cancel it before its request is sent, or
/// check for approval at once. One control covers coven's join and any
/// provider sign-in bae runs for it.
pub struct JoinControl {
    sign_in: watch::Sender<bool>,
    join: coven::JoinControl,
}

impl JoinControl {
    pub fn new() -> Self {
        Self { sign_in: watch::channel(false).0, join: coven::JoinControl::new() }
    }

    /// Cancel the join. Once its request may have been sent, coven keeps the
    /// join and the call ends with `BootstrapError::JoinRequestSent`.
    pub fn cancel(&self) {
        self.sign_in.send_replace(true);
        self.join.cancel();
    }

    /// End the join's wait so the next check for approval runs now.
    pub fn check_now(&self) {
        self.join.check_now();
    }
}

/// Whether a join's cancel can still take the join back at `status`: until
/// its request may have been sent, which is while it opens its database and
/// while it connects. After that coven keeps the join and the cancel ends in
/// `BootstrapError::JoinRequestSent`.
pub fn join_cancellable(status: &coven::BootstrapStatus) -> bool {
    matches!(status, coven::BootstrapStatus::Opening(_) | coven::BootstrapStatus::Connecting)
}

impl Default for JoinControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Join a library from an invite. The person's profile takes the name the
/// admin typed in the invite (§5.4). Dropping the call keeps the join for
/// `resume_join`. An invite that has ended by `clock`, with no join for its
/// library under way here, ends the call with `None` before any sign-in:
/// coven sends no first request after the invite ends and would end the join
/// as soon as it was signed in. A join under way may have sent its request,
/// which coven goes on waiting for (§5.6).
pub async fn join_with_invite(
    mut builder: coven::CovenBuilder,
    code: &str,
    device_name: &str,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    let invite = invite_info(code)?;
    if clock.now() >= invite.expires_at
        && !unfinished_joins(&builder, diagnostics)
            .await?
            .iter()
            .any(|join| join.metadata.store_id == invite.metadata.store_id)
    {
        return Ok(None);
    }
    auth::authenticate(
        &mut builder,
        invite.metadata.oauth_access,
        &control.sign_in.subscribe(),
        diagnostics,
    )
    .await?;
    let joined = Box::pin(coven::join_with_invite(
        loading_builder(builder),
        code,
        device_name,
        on_status,
        &control.join,
    ))
    .await
    .map_err(|error| reported_restore_error(diagnostics, error))?;
    Ok(match joined {
        Some(handle) => {
            Some(with_profile(handle, invite.invitee.as_deref(), clock, diagnostics).await)
        }
        None => None,
    })
}

/// The joins this device has under way, each as its saved invite reads.
pub async fn unfinished_joins(
    builder: &coven::CovenBuilder,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Vec<coven::InviteInfo>, RestoreFromCodeError> {
    builder.unfinished_joins().await.map_err(|error| reported_restore_error(diagnostics, error))
}

/// Continue a join from `unfinished_joins` after bae restarted. coven keeps
/// the join's provider sign-in on the device beside it, so this signs in only
/// when coven asks for one: when the kept sign-in is gone, or the provider
/// refuses it. It then signs in with the access the listed join names and
/// resumes once more. Each call consumes its builder, so `builder` makes one
/// for each (§5.6).
pub async fn resume_join(
    builder: impl Fn() -> coven::CovenBuilder,
    join: &coven::InviteInfo,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    let (store, on_status, coven_control) = (join.metadata.store_id, &on_status, &control.join);
    let resume = move |builder: coven::CovenBuilder| {
        Box::pin(loading_builder(builder).resume_join(store, on_status, coven_control))
    };
    let joined = match resume(builder()).await {
        Err(coven::BootstrapError::OAuth(coven::OAuthError::Reauthorize(_))) => {
            let mut signed_in = builder();
            auth::authenticate(
                &mut signed_in,
                join.metadata.oauth_access,
                &control.sign_in.subscribe(),
                diagnostics,
            )
            .await?;
            resume(signed_in).await
        }
        resumed => resumed,
    }
    .map_err(|error| reported_restore_error(diagnostics, error))?;
    Ok(match joined {
        Some(handle) => {
            Some(with_profile(handle, join.invitee.as_deref(), clock, diagnostics).await)
        }
        None => None,
    })
}

/// The restores of a library onto this device that stopped after they
/// recorded themselves, each as its recorded restore code reads.
pub async fn unfinished_restores(
    builder: &coven::CovenBuilder,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Vec<coven::CodeMetadata>, RestoreFromCodeError> {
    builder.unfinished_restores().await.map_err(|error| reported_restore_error(diagnostics, error))
}

/// Finish a restore from `unfinished_restores` after bae restarted, from what
/// coven recorded: the code, the device's name, and on Dropbox and OneDrive
/// the provider sign-in. This signs in only when coven asks for one, when the
/// kept sign-in is gone or the provider refuses it, with the access the
/// listed restore names, and then resumes once more. A cancel ends the
/// restore as `restore_from_code`'s does: before this device registered,
/// coven removes what the restore kept; after, it fails as
/// `RestoreRegistered`, the restore still unfinished. Each call consumes its
/// builder, so `builder` makes one for each.
pub async fn resume_restore(
    builder: impl Fn() -> coven::CovenBuilder,
    restore: &coven::CodeMetadata,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    let (store, on_status) = (restore.store_id, &on_status);
    let resume = move |builder: coven::CovenBuilder| {
        Box::pin(loading_builder(builder).resume_restore(store, on_status, cancel))
    };
    match resume(builder()).await {
        Err(coven::BootstrapError::OAuth(coven::OAuthError::Reauthorize(_))) => {
            let mut signed_in = builder();
            auth::authenticate(&mut signed_in, restore.oauth_access, cancel, diagnostics).await?;
            resume(signed_in).await
        }
        resumed => resumed,
    }
    .map_err(|error| reported_restore_error(diagnostics, error))
}

/// The invite a code holds; a restore code is refused before any sign-in.
fn invite_info(code: &str) -> Result<coven::InviteInfo, RestoreFromCodeError> {
    match coven::decode_code_info(code) {
        Ok(coven::CodeInfo::Invite(invite)) => Ok(invite),
        Ok(coven::CodeInfo::Restore(_)) => {
            Err(restore_error(coven::BootstrapError::Code(coven::CodeError::WrongKind {
                expected: coven::CodeKind::Invite,
                actual: coven::CodeKind::Restore,
            })))
        }
        Err(error) => Err(restore_error(coven::BootstrapError::Code(error))),
    }
}

/// Ask for the joined person's profile with the invite's name; an invite
/// that replaced an existing member's keys names nobody, and asks for
/// nothing.
async fn with_profile(
    handle: coven::CovenHandle,
    invitee: Option<&str>,
    clock: coven::ClockRef,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> JoinedLibrary {
    let profile = match invitee {
        Some(name) => Some(
            bae_people::People::new(bae_store::Store::new(
                handle.clone(),
                clock,
                diagnostics.bug_sink(),
            ))
            .create_profile(name)
            .await,
        ),
        None => None,
    };
    (handle, profile)
}

/// Replace this device after the caller has warned that unsent edits are discarded.
/// Coven checks storage and custody before replacing the installation. Calling
/// it again finishes a reset that stopped after it recorded itself, as
/// `resume_replacement` does; for a restore that stopped there, coven refuses
/// it and changes nothing. The reset loads the whole store from storage, as a restore does,
/// and reports how far it has come to `on_status`. The returned handle is
/// ready to open in bae; there is no post-reset notification.
pub async fn reset_device(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    loading_builder(builder)
        .reset_device(store, device_name, on_status)
        .await
        .map_err(|error| reported_restore_error(diagnostics, error))
}

/// Restore a library this device holds but can no longer send for, because
/// another of the person's devices replaced their keys. The code must be the
/// person's current restore code for this library. Coven loads the library
/// from storage under a new device id beside this device's copy, which stays
/// and opens as before until the load is complete, then takes its place. A
/// restore that fails before it recorded itself leaves this device's copy,
/// and calling again with the same code retries it; one that stopped after
/// its record is finished by calling again, or by `resume_replacement`
/// without the code. For a reset that stopped there, coven refuses it and
/// changes nothing. A cancel after this device registered fails as
/// `RestoreRegistered`, the restore still unfinished. The caller
/// closed the library and warned that edits on this device that haven't
/// synced are discarded. The copy put in place keeps this device's settings
/// (the `preferences` row and its save presets), read from this device's
/// copy first and written into the new one before it takes the old one's
/// place. When this device's custody already holds the code's keys, the call
/// just opens the library, and that open's steps go to `on_open_progress`, as
/// any open's do; every other status goes to `on_status`. `builder` makes
/// each of the two builders the call uses: one opening this device's copy to
/// read its settings, one restoring.
pub async fn restore_replacing_device(
    builder: impl Fn() -> coven::CovenBuilder,
    store: coven::StoreId,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    on_status: impl Fn(coven::BootstrapStatus),
    on_open_progress: impl Fn(coven::Progress<coven::OpenStep>),
    cancel: &watch::Receiver<bool>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    // Coven refuses an invite or another library's code too, but only after
    // the provider sign-in below; refusing it first asks for no sign-in the
    // restore would never use.
    let refused = |error| restore_error(coven::BootstrapError::Code(error));
    let access = match coven::decode_code_info(code).map_err(refused)? {
        coven::CodeInfo::Restore(metadata) if metadata.store_id == store => metadata.oauth_access,
        coven::CodeInfo::Restore(metadata) => {
            return Err(refused(coven::CodeError::WrongStore {
                expected: store,
                actual: metadata.store_id,
            }))
        }
        coven::CodeInfo::Invite { .. } => {
            return Err(refused(coven::CodeError::WrongKind {
                expected: coven::CodeKind::Restore,
                actual: coven::CodeKind::Invite,
            }))
        }
    };
    let carried = settings_of_this_copy(builder(), store, diagnostics).await?;
    let mut builder = builder();
    auth::authenticate(&mut builder, access, cancel, diagnostics).await?;
    let status = |status| match status {
        coven::BootstrapStatus::Opening(progress) => on_open_progress(progress),
        status => on_status(status),
    };
    let prepare = move |sql: coven::BulkSqlContext<'_>| match &carried {
        Some(carried) => Ok(bae_preferences::carry_preferences(&sql, carried)?),
        None => Ok(()),
    };
    loading_builder(builder)
        .restore_device(store, code, s3_credentials, device_name, prepare, status, cancel)
        .await
        .map_err(|error| reported_restore_error(diagnostics, error))
}

/// Finish this device's reset or restore of `store` that stopped after it
/// recorded itself, from what coven kept on the device: with neither the
/// person's restore code nor a new sign-in. The settings a restore carries
/// were written before its record, so finishing writes nothing more. A
/// Dropbox or OneDrive sign-in the provider refuses even after its refresh
/// fails as `OAuthError::Reauthorize`, the replacement still unfinished, and
/// the call that began it, with its sign-in, finishes it then. How far the
/// load has come goes to `on_status`. A failure leaves the replacement
/// unfinished for the next call. A cancel stops only a restore, as
/// `replacement_cancellable` says: before this device registered, coven
/// gives the restore up and this device's copy is the library again; after,
/// it fails as `RestoreRegistered`, the restore still unfinished.
pub async fn resume_replacement(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    on_status: impl Fn(coven::BootstrapStatus),
    cancel: &watch::Receiver<bool>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, RestoreFromCodeError> {
    loading_builder(builder)
        .resume_replacement(store, on_status, cancel)
        .await
        .map_err(|error| reported_restore_error(diagnostics, error))
}

/// Whether a cancel can stop the call finishing a replacement that `by`
/// names: coven honors one finishing a restore, and checks for none once a
/// reset is under way.
pub fn replacement_cancellable(by: coven::ReplacedBy) -> bool {
    match by {
        coven::ReplacedBy::Restore => true,
        coven::ReplacedBy::Reset => false,
    }
}

/// This device's settings in its copy of `store`, which a restore over the
/// copy carries into the one it puts in place. None when a restore of this
/// copy already recorded itself and stopped: its loaded copy got them before
/// the record, and finishing writes nothing more.
async fn settings_of_this_copy(
    builder: coven::CovenBuilder,
    store: coven::StoreId,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<bae_preferences::Preferences>, RestoreFromCodeError> {
    let unread = |error: crate::LibraryError| RestoreFromCodeError::SettingsUnread(Box::new(error));
    let handle = match configure_store(builder, crate::SCHEMA).open(store, |_| ()).await {
        Ok(handle) => handle,
        Err(coven::CovenError::Lock(coven::StoreLockError::ReplacementUnfinished {
            by: coven::ReplacedBy::Restore,
            ..
        })) => return Ok(None),
        Err(error) => {
            let error = coven::task::reported(diagnostics, error);
            return Err(unread(bae_store::DbError::from(error).into()));
        }
    };
    let store = bae_store::Store::new(
        handle.clone(),
        std::sync::Arc::new(coven::SystemClock),
        diagnostics.bug_sink(),
    );
    let settings = bae_preferences::preferences(&store).await;
    let closed = store.close().await;
    let settings = settings.map_err(|error| unread(error.into()))?;
    closed.map_err(|error| unread(error.into()))?;
    Ok(Some(settings))
}

/// `error`, which a coven call failed with, once the bug it carried back, if
/// any, has gone to `diagnostics`.
fn reported_restore_error(
    diagnostics: &bae_diagnostics::Diagnostics,
    error: coven::BootstrapError,
) -> RestoreFromCodeError {
    restore_error(coven::task::reported(diagnostics, error))
}

fn restore_error(error: coven::BootstrapError) -> RestoreFromCodeError {
    match error {
        coven::BootstrapError::Cancelled => RestoreFromCodeError::Cancelled,
        error => RestoreFromCodeError::Restore(Box::new(error)),
    }
}

#[cfg(test)]
#[path = "bootstrap_tests.rs"]
mod tests;
