use std::sync::Mutex as StateMutex;

use tokio::sync::{Mutex, Notify};

use crate::db::Database;
use crate::library::LibraryError;
use crate::sync::membership::pubkey_fingerprint;

/// The library facts a joining device can display before it authorizes a cloud
/// provider or submits its identity to the existing device.
pub struct DevicePairingOfferInfo {
    pub library_name: String,
    pub cloud_provider: crate::config::CloudProvider,
    pub needs_oauth: bool,
    pub expires_at_unix_seconds: i64,
}

impl DevicePairingOfferInfo {
    pub(super) fn from_offer(offer: &coven::DevicePairingOffer) -> Self {
        let cloud_provider = offer.cloud_provider().clone();
        Self {
            library_name: offer.store_name().to_string(),
            needs_oauth: cloud_provider.needs_oauth(),
            cloud_provider,
            expires_at_unix_seconds: offer.expires_at_unix_seconds(),
        }
    }
}

/// A durable joining-device attempt that the onboarding UI can resume after
/// its process or operation object is gone.
pub struct PendingDevicePairingJoinInfo {
    pub pairing_code: String,
    pub offer: DevicePairingOfferInfo,
    pub fingerprint: String,
    pub phase: coven::DevicePairingPhase,
}

pub fn inspect_device_pairing_offer(
    code: &str,
) -> Result<DevicePairingOfferInfo, coven::DevicePairingError> {
    let offer = coven::DevicePairingOffer::decode(code)?;
    Ok(DevicePairingOfferInfo::from_offer(&offer))
}

/// The exact signed identity waiting for the owner to admit it.
pub struct PairingDevice {
    pub fingerprint: String,
    pub email: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum PairingApprovalState {
    Waiting,
    Approving,
    CancellingViaApproval,
    Cancelling,
    CancellationFailed,
    Terminal,
}

#[derive(Debug, PartialEq, Eq)]
enum ApprovalStartError {
    AlreadyApproving,
    Closed,
}

#[derive(Debug, PartialEq, Eq)]
enum CancellationOwner {
    Session,
    Approval,
    Wait,
    None,
}

impl PairingApprovalState {
    fn begin(&mut self) -> Result<(), ApprovalStartError> {
        match self {
            Self::Waiting => {
                *self = Self::Approving;
                Ok(())
            }
            Self::Approving => Err(ApprovalStartError::AlreadyApproving),
            Self::CancellingViaApproval
            | Self::Cancelling
            | Self::CancellationFailed
            | Self::Terminal => Err(ApprovalStartError::Closed),
        }
    }

    fn cancel(&mut self) -> CancellationOwner {
        match self {
            Self::Waiting | Self::CancellationFailed => {
                *self = Self::Cancelling;
                CancellationOwner::Session
            }
            Self::Approving => {
                *self = Self::CancellingViaApproval;
                CancellationOwner::Approval
            }
            Self::CancellingViaApproval | Self::Cancelling => CancellationOwner::Wait,
            Self::Terminal => CancellationOwner::None,
        }
    }

    fn finish(&mut self) {
        *self = Self::Terminal;
    }

    fn approval_failed(&mut self) {
        *self = match self {
            Self::Approving => Self::Waiting,
            Self::CancellingViaApproval => Self::CancellationFailed,
            state => panic!("approval completed from {state:?}"),
        };
    }

    fn cancellation_failed(&mut self) {
        assert_eq!(*self, Self::Cancelling);
        *self = Self::CancellationFailed;
    }
}

/// The admission of one reviewed request: the request itself, how far its
/// approval has gotten, the signal that stops an approval already running, and
/// the wake for whoever waits on that. Approving and cancelling race for the
/// same decision, so both go through here.
struct PairingApproval {
    reviewed_request: Mutex<Option<coven::DevicePairingRequest>>,
    state: StateMutex<PairingApprovalState>,
    state_changed: Notify,
    cancel_approval: tokio::sync::watch::Sender<bool>,
}

impl PairingApproval {
    fn new() -> Self {
        let (cancel_approval, _) = tokio::sync::watch::channel(false);
        Self {
            reviewed_request: Mutex::new(None),
            state: StateMutex::new(PairingApprovalState::Waiting),
            state_changed: Notify::new(),
            cancel_approval,
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, PairingApprovalState> {
        self.state
            .lock()
            .expect("device pairing approval state mutex poisoned")
    }

    /// Record the request the user is being shown. It is the only request
    /// `approve` can admit.
    async fn review(&self, request: coven::DevicePairingRequest) {
        *self.reviewed_request.lock().await = Some(request);
    }

    async fn approve(
        &self,
        database: &Database,
        host: &coven::DevicePairingHost,
        on_progress: &(dyn Fn(coven::AdmittingDeviceJoinProgress) + Send + Sync),
    ) -> Result<coven::DeviceJoinDriveOutcome, LibraryError> {
        let request = self.reviewed_request.lock().await.clone().ok_or_else(|| {
            LibraryError::Validation("no pairing device was reviewed".to_string())
        })?;
        self.state().begin().map_err(|error| match error {
            ApprovalStartError::AlreadyApproving => {
                LibraryError::Validation("device pairing approval is already running".to_string())
            }
            ApprovalStartError::Closed => {
                LibraryError::from(coven::ApproveDevicePairingError::Cancelled)
            }
        })?;
        let result = database
            .approve_device_pairing(
                host,
                &request,
                on_progress,
                self.cancel_approval.subscribe(),
            )
            .await;
        {
            let mut state = self.state();
            if result.is_ok() || matches!(result, Err(coven::ApproveDevicePairingError::Cancelled))
            {
                state.finish();
            } else {
                state.approval_failed();
            }
        }
        self.state_changed.notify_waiters();
        result
            .inspect_err(|error| {
                tracing::error!(?error, "device pairing approval failed");
            })
            .map_err(LibraryError::from)
    }

    async fn cancel(
        &self,
        database: &Database,
        host: &coven::DevicePairingHost,
    ) -> Result<(), LibraryError> {
        let mut signalled_approval = false;
        loop {
            // Registered before the state is read, so a change between the two
            // still wakes this wait.
            let state_changed = self.state_changed.notified();
            let owner = {
                let mut state = self.state();
                if signalled_approval && *state == PairingApprovalState::CancellingViaApproval {
                    CancellationOwner::Wait
                } else {
                    state.cancel()
                }
            };
            match owner {
                CancellationOwner::Session => {
                    let result = database.cancel_device_pairing(host).await;
                    {
                        let mut state = self.state();
                        if result.is_ok() {
                            state.finish();
                        } else {
                            state.cancellation_failed();
                        }
                    }
                    self.state_changed.notify_waiters();
                    return result.map_err(LibraryError::from);
                }
                CancellationOwner::Approval => {
                    self.cancel_approval.send_replace(true);
                    signalled_approval = true;
                }
                CancellationOwner::Wait => state_changed.await,
                CancellationOwner::None => return Ok(()),
            }
        }
    }
}

/// One owner-side pairing attempt: the host that carries it and the admission
/// of the one request that host delivered.
pub struct DevicePairingSession {
    database: Database,
    host: coven::DevicePairingHost,
    approval: PairingApproval,
}

impl DevicePairingSession {
    pub(crate) fn new(database: Database, host: coven::DevicePairingHost) -> Self {
        Self {
            database,
            host,
            approval: PairingApproval::new(),
        }
    }

    pub fn code(&self) -> String {
        self.host.offer().encode()
    }

    pub async fn wait_for_device(&self) -> Result<PairingDevice, LibraryError> {
        let request = self.host.wait_for_request().await?;
        let device = PairingDevice {
            fingerprint: pubkey_fingerprint(request.public_key()),
            email: request.provider_account_email().map(str::to_string),
        };
        self.approval.review(request).await;
        Ok(device)
    }

    pub async fn approve(
        &self,
        on_progress: &(dyn Fn(coven::AdmittingDeviceJoinProgress) + Send + Sync),
    ) -> Result<(), LibraryError> {
        match self
            .approval
            .approve(&self.database, &self.host, on_progress)
            .await?
        {
            coven::DeviceJoinDriveOutcome::Activated(_) => Ok(()),
            coven::DeviceJoinDriveOutcome::Abandoned(abandonment) => {
                tracing::error!(?abandonment, "device join abandoned by the joining device");
                Err(LibraryError::DeviceJoinAbandoned)
            }
        }
    }

    pub async fn cancel(&self) -> Result<(), LibraryError> {
        self.approval.cancel(&self.database, &self.host).await
    }
}

#[cfg(test)]
mod approval_state_tests {
    use super::*;

    #[test]
    fn approval_owns_cleanup_once_it_has_started() {
        let mut state = PairingApprovalState::Waiting;

        state.begin().expect("approval starts");

        assert_eq!(state.cancel(), CancellationOwner::Approval);
        assert_eq!(state, PairingApprovalState::CancellingViaApproval);
        assert_eq!(state.cancel(), CancellationOwner::Wait);
    }

    #[test]
    fn failed_session_cancellation_can_be_retried() {
        let mut state = PairingApprovalState::Waiting;

        assert_eq!(state.cancel(), CancellationOwner::Session);
        state.cancellation_failed();

        assert_eq!(state.cancel(), CancellationOwner::Session);
    }
}
