use tokio::sync::Mutex;

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

/// One owner-side pairing attempt: the host that carries it and the one
/// request it delivered for review.
///
/// Approving and cancelling race for the same decision; coven serializes them
/// on the session. A cancel while an approval runs asks it to unwind and waits
/// for it, and an approval that starts after a cancel was asked for refuses.
pub struct DevicePairingSession {
    database: Database,
    host: coven::DevicePairingHost,
    /// The request the user is being shown: the only one `approve` admits.
    reviewed_request: Mutex<Option<coven::DevicePairingRequest>>,
}

impl DevicePairingSession {
    pub(crate) fn new(database: Database, host: coven::DevicePairingHost) -> Self {
        Self {
            database,
            host,
            reviewed_request: Mutex::new(None),
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
        *self.reviewed_request.lock().await = Some(request);
        Ok(device)
    }

    pub async fn approve(
        &self,
        on_progress: &(dyn Fn(coven::AdmittingDeviceJoinProgress) + Send + Sync),
    ) -> Result<(), LibraryError> {
        let request = self.reviewed_request.lock().await.clone().ok_or_else(|| {
            LibraryError::Validation("no pairing device was reviewed".to_string())
        })?;
        // Cancellation reaches the approval through the session itself
        // (`cancel` below), so this signal never fires.
        let (_no_cancel, cancel) = tokio::sync::watch::channel(false);
        let outcome = self
            .database
            .approve_device_pairing(&self.host, &request, on_progress, cancel)
            .await
            .inspect_err(|error| {
                tracing::error!(?error, "device pairing approval failed");
            })?;
        match outcome {
            coven::DeviceJoinDriveOutcome::Activated(_) => Ok(()),
            coven::DeviceJoinDriveOutcome::Abandoned(abandonment) => {
                tracing::error!(?abandonment, "device join abandoned by the joining device");
                Err(LibraryError::DeviceJoinAbandoned)
            }
        }
    }

    pub async fn cancel(&self) -> Result<(), LibraryError> {
        Ok(self.database.cancel_device_pairing(&self.host).await?)
    }
}
