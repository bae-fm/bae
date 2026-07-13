//! The configuration a manual cloud restore needs, and the rule for when it is
//! complete.
//!
//! A restore is driven by a form: the user picks a provider, types the library id
//! and encryption key, fills in that provider's fields, and authorizes if the
//! provider uses OAuth. Two questions follow from one value — "may the user press
//! Restore yet" and "restore this" — so both are answered from
//! [`RestoreConfig`]: [`RestoreConfig::validate`] gates the button,
//! [`RestoreConfig::into_home`] performs the conversion into coven's cloud-home
//! join info. Modelling the form and the restore separately lets them disagree
//! about what a provider requires; there is one model here so they cannot.

use coven::{CloudHomeJoinInfo, OAuthTokens};

/// A manual restore configuration: which library, the key that decrypts it, and
/// where its cloud home lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreConfig {
    pub library_id: String,
    pub encryption_key: String,
    pub home: RestoreHome,
}

/// Where a restored library's cloud home lives, with the credentials that reach
/// it.
///
/// The OAuth providers carry their token inline: `None` means the user has not
/// authorized yet, which [`RestoreConfig::validate`] rejects. It is a field of the
/// home rather than a separate "has a token" flag, so the value that gates the
/// form is the same value that performs the restore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreHome {
    S3 {
        bucket: String,
        region: String,
        /// A custom S3-compatible endpoint. Absent means AWS itself.
        endpoint: Option<String>,
        access_key: String,
        secret_key: String,
    },
    CloudKit,
    GoogleDrive {
        folder_id: String,
        oauth_token_json: Option<String>,
    },
    Dropbox {
        folder_path: String,
        oauth_token_json: Option<String>,
    },
    OneDrive {
        drive_id: String,
        folder_id: String,
        oauth_token_json: Option<String>,
    },
}

/// Why a [`RestoreConfig`] can't be restored from. The form renders this to say
/// what is still missing; the restore call rejects on it, so a surface with no
/// form cannot write past the rule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreConfigError {
    #[error("A library ID is required")]
    MissingLibraryId,
    #[error("An encryption key is required")]
    MissingEncryptionKey,
    /// A provider-specific field the chosen provider needs — the bucket for S3,
    /// the folder for Google Drive, and so on.
    #[error("{0} is required")]
    MissingField(&'static str),
    #[error("Authorize with the cloud provider to continue")]
    MissingOauthToken,
    #[error("The authorization from the cloud provider could not be read: {0}")]
    InvalidOauthToken(String),
}

impl RestoreConfig {
    /// Whether every field the chosen provider needs is filled in. The one
    /// definition of "this restore configuration is complete".
    pub fn validate(&self) -> Result<(), RestoreConfigError> {
        if self.library_id.trim().is_empty() {
            return Err(RestoreConfigError::MissingLibraryId);
        }
        if self.encryption_key.trim().is_empty() {
            return Err(RestoreConfigError::MissingEncryptionKey);
        }
        self.home.validate()
    }

    /// coven's cloud-home join info and the parsed OAuth tokens for this config,
    /// after checking it against [`validate`](Self::validate).
    ///
    /// The CloudKit ops driver is not produced here: it is a platform object the
    /// caller holds, and it attaches that to the `RestoreSource` itself.
    pub fn into_home(self) -> Result<(CloudHomeJoinInfo, Option<OAuthTokens>), RestoreConfigError> {
        self.validate()?;
        self.home.into_join_info()
    }
}

/// A required text field: present once it holds more than whitespace.
fn require(value: &str, field: &'static str) -> Result<(), RestoreConfigError> {
    if value.trim().is_empty() {
        Err(RestoreConfigError::MissingField(field))
    } else {
        Ok(())
    }
}

/// An OAuth provider's token: the user has authorized once it holds anything.
fn require_token(token: &Option<String>) -> Result<(), RestoreConfigError> {
    match token {
        Some(token) if !token.trim().is_empty() => Ok(()),
        _ => Err(RestoreConfigError::MissingOauthToken),
    }
}

/// Parse an authorized provider's token payload. Absent is unreachable after
/// `validate`, and is reported as the missing-authorization it is.
fn parse_token(token: Option<String>) -> Result<Option<OAuthTokens>, RestoreConfigError> {
    let token = token.ok_or(RestoreConfigError::MissingOauthToken)?;
    serde_json::from_str(&token)
        .map(Some)
        .map_err(|e| RestoreConfigError::InvalidOauthToken(e.to_string()))
}

impl RestoreHome {
    fn validate(&self) -> Result<(), RestoreConfigError> {
        match self {
            // `endpoint` is genuinely optional — absent means AWS itself.
            RestoreHome::S3 {
                bucket,
                region,
                access_key,
                secret_key,
                endpoint: _,
            } => {
                require(bucket, "A bucket")?;
                require(region, "A region")?;
                require(access_key, "An access key")?;
                require(secret_key, "A secret key")
            }
            // The library id and encryption key are all CloudKit needs; the
            // container is the app's own.
            RestoreHome::CloudKit => Ok(()),
            RestoreHome::GoogleDrive {
                folder_id,
                oauth_token_json,
            } => {
                require(folder_id, "A folder ID")?;
                require_token(oauth_token_json)
            }
            RestoreHome::Dropbox {
                folder_path,
                oauth_token_json,
            } => {
                require(folder_path, "A folder path")?;
                require_token(oauth_token_json)
            }
            RestoreHome::OneDrive {
                drive_id,
                folder_id,
                oauth_token_json,
            } => {
                require(drive_id, "A drive ID")?;
                require(folder_id, "A folder ID")?;
                require_token(oauth_token_json)
            }
        }
    }

    fn into_join_info(
        self,
    ) -> Result<(CloudHomeJoinInfo, Option<OAuthTokens>), RestoreConfigError> {
        Ok(match self {
            RestoreHome::S3 {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
            } => (
                // A restore names the bucket the library was written to, so it
                // reads from that bucket's root.
                CloudHomeJoinInfo::S3 {
                    bucket,
                    region,
                    endpoint,
                    access_key,
                    secret_key,
                    key_prefix: None,
                },
                None,
            ),
            RestoreHome::CloudKit => (CloudHomeJoinInfo::CloudKit, None),
            RestoreHome::GoogleDrive {
                folder_id,
                oauth_token_json,
            } => (
                CloudHomeJoinInfo::GoogleDrive { folder_id },
                parse_token(oauth_token_json)?,
            ),
            RestoreHome::Dropbox {
                folder_path,
                oauth_token_json,
            } => (
                CloudHomeJoinInfo::Dropbox { folder_path },
                parse_token(oauth_token_json)?,
            ),
            RestoreHome::OneDrive {
                drive_id,
                folder_id,
                oauth_token_json,
            } => (
                CloudHomeJoinInfo::OneDrive {
                    drive_id,
                    folder_id,
                },
                parse_token(oauth_token_json)?,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(home: RestoreHome) -> RestoreConfig {
        RestoreConfig {
            library_id: "lib-1".to_string(),
            encryption_key: "deadbeef".to_string(),
            home,
        }
    }

    fn s3() -> RestoreHome {
        RestoreHome::S3 {
            bucket: "bucket".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key: "access".to_string(),
            secret_key: "secret".to_string(),
        }
    }

    #[test]
    fn a_filled_in_s3_config_is_complete() {
        assert_eq!(config(s3()).validate(), Ok(()));
    }

    #[test]
    fn s3_needs_every_credential() {
        let RestoreHome::S3 { region, .. } = s3() else {
            unreachable!()
        };
        let missing_bucket = RestoreHome::S3 {
            bucket: "  ".to_string(),
            region,
            endpoint: None,
            access_key: "access".to_string(),
            secret_key: "secret".to_string(),
        };
        assert_eq!(
            config(missing_bucket).validate(),
            Err(RestoreConfigError::MissingField("A bucket")),
        );
    }

    /// The endpoint is the one S3 field a user may leave blank — it means AWS.
    #[test]
    fn s3_does_not_require_an_endpoint() {
        assert_eq!(config(s3()).validate(), Ok(()));
    }

    #[test]
    fn the_library_id_and_key_are_required_whatever_the_provider() {
        let mut c = config(RestoreHome::CloudKit);
        c.library_id = "  ".to_string();
        assert_eq!(c.validate(), Err(RestoreConfigError::MissingLibraryId));

        let mut c = config(RestoreHome::CloudKit);
        c.encryption_key = String::new();
        assert_eq!(c.validate(), Err(RestoreConfigError::MissingEncryptionKey));
    }

    /// CloudKit's container is the app's own, so it asks for nothing else.
    #[test]
    fn cloudkit_needs_nothing_beyond_the_library_and_key() {
        assert_eq!(config(RestoreHome::CloudKit).validate(), Ok(()));
    }

    #[test]
    fn an_oauth_provider_is_incomplete_until_it_is_authorized() {
        let unauthorized = RestoreHome::GoogleDrive {
            folder_id: "folder".to_string(),
            oauth_token_json: None,
        };
        assert_eq!(
            config(unauthorized).validate(),
            Err(RestoreConfigError::MissingOauthToken),
        );

        let authorized = RestoreHome::GoogleDrive {
            folder_id: "folder".to_string(),
            oauth_token_json: Some(r#"{"access_token":"t"}"#.to_string()),
        };
        assert_eq!(config(authorized).validate(), Ok(()));
    }

    /// Authorizing doesn't excuse the provider's own fields.
    #[test]
    fn an_authorized_oauth_provider_still_needs_its_folder() {
        let missing_folder = RestoreHome::Dropbox {
            folder_path: String::new(),
            oauth_token_json: Some(r#"{"access_token":"t"}"#.to_string()),
        };
        assert_eq!(
            config(missing_folder).validate(),
            Err(RestoreConfigError::MissingField("A folder path")),
        );
    }

    #[test]
    fn one_drive_needs_both_its_drive_and_its_folder() {
        let missing_drive = RestoreHome::OneDrive {
            drive_id: String::new(),
            folder_id: "folder".to_string(),
            oauth_token_json: Some(r#"{"access_token":"t"}"#.to_string()),
        };
        assert_eq!(
            config(missing_drive).validate(),
            Err(RestoreConfigError::MissingField("A drive ID")),
        );
    }

    /// An incomplete config never reaches coven.
    #[test]
    fn into_home_refuses_an_incomplete_config() {
        let unauthorized = RestoreHome::Dropbox {
            folder_path: "folder".to_string(),
            oauth_token_json: None,
        };
        assert_eq!(
            config(unauthorized).into_home().unwrap_err(),
            RestoreConfigError::MissingOauthToken,
        );
    }

    #[test]
    fn into_home_carries_s3_credentials_through_to_coven() {
        let (join_info, tokens) = config(s3()).into_home().expect("complete config");
        assert!(tokens.is_none());
        match join_info {
            CloudHomeJoinInfo::S3 {
                bucket,
                region,
                endpoint,
                key_prefix,
                ..
            } => {
                assert_eq!(bucket, "bucket");
                assert_eq!(region, "us-east-1");
                assert_eq!(endpoint, None);
                assert_eq!(key_prefix, None);
            }
            other => panic!("expected S3 join info, got {other:?}"),
        }
    }

    #[test]
    fn a_malformed_authorization_payload_is_reported_as_such() {
        let bad = RestoreHome::GoogleDrive {
            folder_id: "folder".to_string(),
            oauth_token_json: Some("not json".to_string()),
        };
        assert!(matches!(
            config(bad).into_home().unwrap_err(),
            RestoreConfigError::InvalidOauthToken(_),
        ));
    }
}
