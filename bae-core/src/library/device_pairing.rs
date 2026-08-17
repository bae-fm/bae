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

pub fn inspect_device_pairing_offer(
    code: &str,
) -> Result<DevicePairingOfferInfo, coven::DevicePairingError> {
    let offer = coven::DevicePairingOffer::decode(code)?;
    let cloud_provider = offer.cloud_provider().clone();
    Ok(DevicePairingOfferInfo {
        library_name: offer.store_name().to_string(),
        needs_oauth: cloud_provider.needs_oauth(),
        cloud_provider,
        expires_at_unix_seconds: offer.expires_at_unix_seconds(),
    })
}

/// The exact signed identity waiting for the owner to admit it.
pub struct PairingDevice {
    pub fingerprint: String,
    pub email: Option<String>,
}

/// One owner-side pairing attempt. The request shown to the user stays inside
/// this object and is the only request approval can admit.
pub struct DevicePairingSession {
    database: Database,
    host: coven::DevicePairingHost,
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

    pub async fn approve(&self) -> Result<(), LibraryError> {
        let request = self.reviewed_request.lock().await.clone().ok_or_else(|| {
            LibraryError::Validation("no pairing device was reviewed".to_string())
        })?;
        let outcome = self
            .database
            .approve_device_pairing(&self.host, &request)
            .await?;
        match outcome {
            coven::DeviceJoinDriveOutcome::Activated(_) => Ok(()),
            coven::DeviceJoinDriveOutcome::Abandoned(_) => Err(LibraryError::DeviceJoinAbandoned),
        }
    }

    pub fn cancel(&self) -> Result<(), LibraryError> {
        self.host.cancel()?;
        self.host.finish()?;
        Ok(())
    }
}
