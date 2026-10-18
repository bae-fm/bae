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

pub(super) fn code_access(code: &str) -> Result<Option<coven::OAuthAccess>, RestoreFromCodeError> {
    coven::decode_code_info(code)
        .map(|info| info.metadata().oauth_access)
        .map_err(|error| restore_error(coven::BootstrapError::Code(error)))
}
