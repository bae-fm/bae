use super::*;

/// Credentials belong to this operation and are never stored by the bridge.
#[derive(Clone, uniffi::Enum)]
pub enum BridgeAdminAccess {
    AccessKey {
        access_key_id: String,
        secret_access_key: String,
        session_token: Option<String>,
        iam_endpoint: Option<String>,
    },
    BearerToken {
        token: String,
    },
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AwsCli {
        profile: Option<String>,
    },
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    GcloudCli {
        account: Option<String>,
    },
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    GoogleSignIn,
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AwsIdentityCenter {
        start_url: String,
        region: String,
        account_id: String,
        role_name: String,
    },
}
impl BridgeAdminAccess {
    pub(crate) fn into_core(self) -> Result<coven::AdminAccessSource, BridgeError> {
        use coven::{AdminAccessSource as S, AdminCredential as C, SecretText};
        Ok(match self {
            Self::AccessKey {
                access_key_id,
                secret_access_key,
                session_token,
                iam_endpoint,
            } => S::Pasted(C::AccessKey {
                access_key_id,
                secret_access_key: SecretText::new(secret_access_key),
                session_token: session_token.map(SecretText::new),
                iam_endpoint: iam_endpoint
                    .map(|url| url.parse())
                    .transpose()
                    .map_err(BridgeError::config)?,
            }),
            Self::BearerToken { token } => S::Pasted(C::BearerToken(SecretText::new(token))),
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            Self::AwsCli { profile } => S::AwsCli { profile },
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            Self::GcloudCli { account } => S::GcloudCli { account },
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            Self::GoogleSignIn => S::GoogleSignIn,
            #[cfg(not(any(target_os = "ios", target_os = "android")))]
            Self::AwsIdentityCenter {
                start_url,
                region,
                account_id,
                role_name,
            } => S::AwsIdentityCenter {
                start_url,
                region,
                account_id,
                role_name,
                presenter: std::sync::Arc::new(coven::DesktopDeviceSignIn),
            },
        })
    }
}
