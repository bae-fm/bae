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
    #[error("restore cancelled")]
    Cancelled,
    #[error("restore failed: {0}")]
    Restore(#[source] Box<coven::BootstrapError>),
}

impl RestoreFromCodeError {
    /// What the person can do about the failure: fix the code, the cloud
    /// credentials, or the keyring, or retry once the cloud is reachable.
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Cancelled => bae_diagnostics::UiErrorCategory::Internal,
            Self::Restore(error) => bae_diagnostics::bootstrap_category(error),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CreateLibraryError {
    #[error(transparent)]
    Store(#[from] coven::StoreCreationError),
    /// The creator's name couldn't be kept, so the library was not kept.
    #[error("keeping the creator's name: {0}")]
    CreatorName(#[source] bae_store::DbError),
    /// The library made, and the creator's name not kept, couldn't be taken
    /// back: the store stays on this device.
    #[error("keeping the creator's name: {name}; and the new library was not removed: {removal}")]
    NotRemoved { name: bae_store::DbError, removal: coven::StoreDeletionError },
}

impl CreateLibraryError {
    pub fn category(&self) -> bae_diagnostics::UiErrorCategory {
        match self {
            Self::Store(error) => bae_diagnostics::creation_category(error),
            Self::CreatorName(error) | Self::NotRemoved { name: error, .. } => {
                bae_diagnostics::database_category(error)
            }
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

/// Create a store named `name`, and keep the name of its creator, `creator`,
/// for their profile: the creator is no member until storage is set up, so
/// the library asks for their profile when it has one (§5.4). Retains the
/// store's open handle, including its identity custody. If the name can't be
/// kept, the new library is deleted again and the call fails: a library
/// either keeps its creator's name or doesn't exist.
pub async fn create_library(
    builder: coven::CovenBuilder,
    name: bae_store::EnteredName,
    creator: bae_store::EnteredName,
    clock: coven::ClockRef,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<coven::CovenHandle, CreateLibraryError> {
    create_keeping(builder, name, clock, diagnostics, |store| async move {
        bae_people::People::new(store).keep_creator_name(&creator).await
    })
    .await
}

/// [`create_library`] with the write that keeps the creator's name given, so
/// a test can make it fail.
async fn create_keeping<Keep, Kept>(
    builder: coven::CovenBuilder,
    name: bae_store::EnteredName,
    clock: coven::ClockRef,
    diagnostics: &bae_diagnostics::Diagnostics,
    keep: Keep,
) -> Result<coven::CovenHandle, CreateLibraryError>
where
    Keep: FnOnce(bae_store::Store) -> Kept,
    Kept: std::future::Future<Output = Result<(), bae_store::DbError>>,
{
    let handle = configure_store(builder, crate::SCHEMA)
        .create_store(name.as_str())
        .await
        .map_err(|error| CreateLibraryError::Store(coven::task::reported(diagnostics, error)))?;
    let store = bae_store::Store::new(handle.clone(), clock, diagnostics.bug_sink());
    match keep(store.clone()).await {
        Ok(()) => Ok(handle),
        Err(name) => match store.delete_store().await {
            Ok(()) => Err(CreateLibraryError::CreatorName(name)),
            Err(removal) => Err(CreateLibraryError::NotRemoved { name, removal }),
        },
    }
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
    let code = &crate::sync::restore_code::entered(code);
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

/// A join that let this device in: the library's open handle, and what
/// failed once coven published it, if anything. The person's profile is
/// asked for when the library opens, under the name coven keeps from the
/// invite (§5.4).
pub enum JoinedLibrary {
    /// The library loaded.
    Loaded(coven::CovenHandle),
    /// The library is open, and `failure` failed once coven had published
    /// it: its first load (lost storage access among the causes), or
    /// keeping the published library. Its sync goes on from where the load
    /// stopped; its sync state shows a load that can't reach storage (§5.6).
    Published { handle: coven::CovenHandle, failure: Box<coven::BootstrapError> },
}

/// How often this device checks while a screen waits on another device: a
/// join's waiting screen for its approval, and Add Device's sheet for the new
/// device. A quiet check is two requests: about 24 a minute, only while the
/// screen shows.
pub(crate) const WAITING_SCREEN_CHECK_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(5);

/// The app's hold on a running join: cancel it before its request is sent, or
/// check for approval often while the waiting screen shows. One control
/// covers coven's join and any provider sign-in bae runs for it.
pub struct JoinControl {
    sign_in: watch::Sender<bool>,
    join: coven::JoinControl,
    /// The hold on coven's checks while the waiting screen shows.
    often: std::sync::Mutex<Option<coven::SyncOftenGuard>>,
}

impl JoinControl {
    pub fn new() -> Self {
        Self {
            sign_in: watch::channel(false).0,
            join: coven::JoinControl::new(),
            often: std::sync::Mutex::new(None),
        }
    }

    /// Cancel the join. Once its request may have been sent, coven keeps the
    /// join and the call ends with `BootstrapError::JoinRequestSent`; once
    /// the join has found its approval, the cancel changes nothing, and the
    /// call opens the library.
    pub fn cancel(&self) {
        self.sign_in.send_replace(true);
        self.join.cancel();
    }

    /// Whether the join's waiting screen shows. While it does, the join
    /// checks for approval every [`WAITING_SCREEN_CHECK_INTERVAL`] from the
    /// end of its last check, and otherwise on coven's doubling waits.
    pub fn set_screen_shown(&self, shown: bool) {
        let mut often = self.often.lock().expect("the check hold's lock is never poisoned");
        match (shown, often.is_some()) {
            (true, false) => *often = Some(self.join.check_often(WAITING_SCREEN_CHECK_INTERVAL)),
            (false, true) => *often = None,
            (true, true) | (false, false) => {}
        }
    }
}

#[cfg(test)]
impl JoinControl {
    /// Whether the control holds coven's checks often now.
    fn holds_checks_often(&self) -> bool {
        self.often.lock().unwrap().is_some()
    }
}

/// Whether a join's cancel can still take the join back at `status`: until
/// its request may have been sent, which is while it opens its database and
/// while it connects. After that coven keeps the join and the cancel ends in
/// `BootstrapError::JoinRequestSent`, and once the join has found its
/// approval, publishing and loading the library, nothing stops it.
pub fn join_cancellable(status: &coven::BootstrapStatus) -> bool {
    matches!(status, coven::BootstrapStatus::Opening(_) | coven::BootstrapStatus::Connecting)
}

/// Join a library from an invite as the person named `joiner`, the name they
/// typed, which the join's request carries and coven keeps with the library,
/// which the library's open asks for the person's profile under (§5.4).
/// Dropping the call before the join finds
/// its approval keeps the join for `resume_join`; once it has, coven
/// publishes the library, which then loads, and the call returns it open
/// even when its load failed. An invite that has ended by `clock`, with no
/// join for its library under way here, ends the call with `None` before
/// any sign-in: coven sends no first request after the invite ends and would
/// end the join as soon as it was signed in. A join under way may have sent
/// its request, which coven goes on waiting for (§5.6).
pub async fn join_with_invite(
    mut builder: coven::CovenBuilder,
    code: &str,
    joiner: bae_store::EnteredName,
    device_name: &str,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    let code = &crate::sync::restore_code::entered(code);
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
        Some(joiner.into_string()),
        device_name,
        on_status,
        &control.join,
    ))
    .await;
    admitted(joined, diagnostics)
}

/// Add this device to the person's own library from a pairing code another of
/// their devices shows. The call returns once that device approves the
/// request, with the library open. `None` ends it with no library: the
/// pairing ended first (declined, closed, lost or expired), as the code's end
/// by `clock` does before any sign-in, since coven sends no first request
/// after it. `s3_credentials` are the key the person typed, for a code that
/// carries none; the QR code carries the key. Cancelling through `control`,
/// dropping the call or failing it removes this device's copy, whether or not
/// the request was sent: approval binds nothing until the device registers
/// (§13.2).
pub async fn join_with_pairing(
    mut builder: coven::CovenBuilder,
    code: &str,
    s3_credentials: Option<coven::S3Credentials>,
    device_name: &str,
    clock: coven::ClockRef,
    on_status: impl Fn(coven::BootstrapStatus),
    control: &JoinControl,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<coven::CovenHandle>, RestoreFromCodeError> {
    let code = &crate::sync::restore_code::entered(code);
    let pairing = pairing_info(code)?;
    if clock.now() >= pairing.expires_at {
        return Ok(None);
    }
    auth::authenticate(
        &mut builder,
        pairing.metadata.oauth_access,
        &control.sign_in.subscribe(),
        diagnostics,
    )
    .await?;
    Box::pin(coven::join_with_pairing(
        loading_builder(builder),
        code,
        s3_credentials,
        device_name,
        on_status,
        &control.join,
    ))
    .await
    .map_err(|error| reported_restore_error(diagnostics, error))
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
    };
    admitted(joined, diagnostics)
}

/// The restores of a library onto this device that stopped after they
/// recorded themselves, each as its recorded restore code reads.
pub async fn unfinished_restores(
    builder: &coven::CovenBuilder,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Vec<coven::CodeMetadata>, RestoreFromCodeError> {
    builder.unfinished_restores().await.map_err(|error| reported_restore_error(diagnostics, error))
}

/// Delete `store`'s unfinished first installation from this device: the
/// directory it kept and this device's entries for it. For a join or restore
/// that can't resume, which `unfinished_joins` and `unfinished_restores` fail
/// on, naming it in `BootstrapError::Directory(Settings { store, .. })`
/// until it is gone. A join whose request was sent stays pending in the
/// library's storage until its invite ends, and the person's saved restore
/// code stays. Fails as `NoUnfinishedInstallation` when this device has none
/// for `store`, or a call holds it, and deletes nothing.
pub async fn discard_unfinished_installation(
    builder: &coven::CovenBuilder,
    store: coven::StoreId,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<(), RestoreFromCodeError> {
    builder
        .discard_unfinished_installation(store)
        .await
        .map_err(|error| reported_restore_error(diagnostics, error))
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
        Ok(coven::CodeInfo::Pairing(_)) => {
            Err(restore_error(coven::BootstrapError::Code(coven::CodeError::WrongKind {
                expected: coven::CodeKind::Invite,
                actual: coven::CodeKind::Pairing,
            })))
        }
        Err(error) => Err(restore_error(coven::BootstrapError::Code(error))),
    }
}

fn pairing_info(code: &str) -> Result<coven::PairingInfo, RestoreFromCodeError> {
    let wrong = |actual| {
        restore_error(coven::BootstrapError::Code(coven::CodeError::WrongKind {
            expected: coven::CodeKind::Pairing,
            actual,
        }))
    };
    match coven::decode_code_info(code) {
        Ok(coven::CodeInfo::Pairing(pairing)) => Ok(pairing),
        Ok(coven::CodeInfo::Restore(_)) => Err(wrong(coven::CodeKind::Restore)),
        Ok(coven::CodeInfo::Invite(_)) => Err(wrong(coven::CodeKind::Invite)),
        Err(error) => Err(restore_error(coven::BootstrapError::Code(error))),
    }
}

/// What a join call came to, once coven answered it: the library it let
/// this device into, or none when the invite ended without letting it in. A
/// library coven published is the join's however its first load went: coven
/// lists it as no join, and its sync goes on with the load (§5.6).
fn admitted(
    joined: Result<Option<coven::CovenHandle>, coven::BootstrapError>,
    diagnostics: &bae_diagnostics::Diagnostics,
) -> Result<Option<JoinedLibrary>, RestoreFromCodeError> {
    match joined.map_err(|error| coven::task::reported(diagnostics, error)) {
        Ok(None) => Ok(None),
        Ok(Some(handle)) => Ok(Some(JoinedLibrary::Loaded(handle))),
        Err(coven::BootstrapError::Published { handle, source }) => {
            tracing::warn!(
                "the joined library is published and open, after a failure: {}",
                bae_util::error_text::CauseChain(&*source)
            );
            Ok(Some(JoinedLibrary::Published { handle, failure: source }))
        }
        Err(error) => Err(restore_error(error)),
    }
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
/// synced are discarded. The copy put in place starts at a new library's
/// settings, as a reset's does: nothing carries the settings of the copy it
/// replaces. When this device's custody already holds the code's keys, the
/// call just opens the library, and that open's steps go to `on_open_progress`,
/// as any open's do; every other status goes to `on_status`.
pub async fn restore_replacing_device(
    mut builder: coven::CovenBuilder,
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
    let code = &crate::sync::restore_code::entered(code);
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
        coven::CodeInfo::Pairing(_) => {
            return Err(refused(coven::CodeError::WrongKind {
                expected: coven::CodeKind::Restore,
                actual: coven::CodeKind::Pairing,
            }))
        }
    };
    auth::authenticate(&mut builder, access, cancel, diagnostics).await?;
    let status = |status| match status {
        coven::BootstrapStatus::Opening(progress) => on_open_progress(progress),
        status => on_status(status),
    };
    loading_builder(builder)
        .restore_device(store, code, s3_credentials, device_name, |_| Ok(()), status, cancel)
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
