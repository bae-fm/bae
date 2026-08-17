use super::*;

#[derive(uniffi::Object)]
pub struct BridgeDevicePairingSession {
    inner: std::sync::Arc<bae_core::library::DevicePairingSession>,
    runtime: tokio::runtime::Handle,
}

#[uniffi::export]
impl AppHandle {
    pub async fn start_device_pairing(
        self: std::sync::Arc<Self>,
    ) -> Result<std::sync::Arc<BridgeDevicePairingSession>, BridgeError> {
        self.run_exported(move |this| async move {
            let session = this.services.start_device_pairing().await?;
            Ok(std::sync::Arc::new(BridgeDevicePairingSession {
                inner: std::sync::Arc::new(session),
                runtime: this.runtime.handle().clone(),
            }))
        })
        .await
    }
}

#[uniffi::export(async_runtime = "tokio", cancellable)]
impl BridgeDevicePairingSession {
    pub fn code(&self) -> String {
        self.inner.code()
    }

    pub async fn wait_for_device(
        self: std::sync::Arc<Self>,
    ) -> Result<BridgePairingDevice, BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run(runtime, move || async move {
            let device = self.inner.wait_for_device().await?;
            Ok(BridgePairingDevice {
                fingerprint: device.fingerprint,
                email: device.email,
            })
        })
        .await
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl BridgeDevicePairingSession {
    pub async fn approve(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run_to_completion(
            runtime,
            "device pairing approval",
            move || async move {
                self.inner.approve().await?;
                Ok(())
            },
        )
        .await
    }

    pub async fn cancel(self: std::sync::Arc<Self>) -> Result<(), BridgeError> {
        let runtime = self.runtime.clone();
        crate::operation_runtime::run_to_completion(
            runtime,
            "device pairing cancellation",
            move || async move {
                self.inner.cancel().await?;
                Ok(())
            },
        )
        .await
    }
}
