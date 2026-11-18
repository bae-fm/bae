//! The app's sheet that a cast asks for an AirPlay receiver's code through.

use bae_util::task_failure::Bug;

use crate::types::BridgeAirPlayCodeRequest;

/// The app's sheet that asks for a receiver's code. Each call shows
/// `request` and answers with the code the person entered; the sheet keeps
/// what they typed between calls. Cancelling the foreign future, which a cast
/// that ends while the call waits does, closes the sheet.
#[uniffi::export(callback_interface)]
#[async_trait::async_trait]
pub trait AirPlayCodePromptCallback: Send + Sync {
    async fn code(&self, request: BridgeAirPlayCodeRequest) -> String;
}

/// The app's sheet, as the cast asks it.
pub(crate) struct NativeCodePrompt {
    callback: Box<dyn AirPlayCodePromptCallback>,
    /// Where a throw from the sheet is reported.
    diagnostics: bae_diagnostics::Diagnostics,
}

impl NativeCodePrompt {
    pub(crate) fn new(
        callback: Box<dyn AirPlayCodePromptCallback>,
        diagnostics: bae_diagnostics::Diagnostics,
    ) -> Self {
        Self { callback, diagnostics }
    }
}

#[async_trait::async_trait]
impl bae_cast::CodePrompt for NativeCodePrompt {
    async fn code(&self, request: bae_cast::CodeRequest) -> Result<String, Bug> {
        crate::host_calls::host_call_async(
            |bug| self.diagnostics.panicked(bug),
            "ask for an AirPlay receiver's code",
            self.callback.code(BridgeAirPlayCodeRequest::from_core(request)),
        )
        .await
    }
}

#[cfg(test)]
#[path = "airplay_prompt_tests.rs"]
mod tests;
