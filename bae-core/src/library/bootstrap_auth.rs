use super::*;

pub(super) async fn authenticate(
    builder: &mut coven::CovenBuilder,
    access: Option<coven::OAuthAccess>,
    cancel: &watch::Receiver<bool>,
) -> Result<(), RestoreFromCodeError> {
    let Some(access) = access else {
        return Ok(());
    };
    let mut cancel = cancel.clone();
    if *cancel.borrow() {
        return Err(RestoreFromCodeError::Cancelled);
    }
    tokio::select! {
        result = builder.authenticate(access) => result.map_err(|error| match error {
            coven::OAuthError::Cancelled => RestoreFromCodeError::Cancelled,
            error => restore_error(coven::BootstrapError::OAuth(error)),
        }),
        _ = cancel.wait_for(|cancelled| *cancelled) => Err(RestoreFromCodeError::Cancelled),
    }
}

/// The store and sign-in either kind of code names.
pub(super) fn code_metadata(code: &str) -> Result<coven::CodeMetadata, RestoreFromCodeError> {
    coven::decode_code_info(code)
        .map(|info| info.metadata().clone())
        .map_err(|error| restore_error(coven::BootstrapError::Code(error)))
}
