//! The profile a debug build's dev reset recreates its library from: the
//! library's name, the S3 storage it connects to, the Discogs token, and the
//! two metadata preferences. The file holds credentials, so it is read only
//! when no one but its owner can read or write it, and no error repeats its
//! contents. Emptying that storage's prefix, so the recreated library starts
//! on an empty remote, is here too.

use crate::library_name::{EmptyLibraryName, LibraryName};
use crate::sync::S3ConfigData;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A dev library's profile, read from its TOML file.
pub struct DevResetProfile {
    pub library_name: LibraryName,
    /// The storage setup the library connects to, with the profile coven
    /// picks from the endpoint, as the storage setup form's default does.
    pub s3: S3ConfigData,
    pub discogs_token: String,
    pub identify_automatically: bool,
    pub import_when_identified: bool,
}

/// Why a profile couldn't be read. None of these carries a value from the
/// file, since any of them could be a credential, except the folder of a
/// refused key prefix: that names a place in the bucket, never a secret.
#[derive(Debug, thiserror::Error)]
pub enum DevResetProfileError {
    #[error("read dev profile {path}: {error}")]
    Read {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },
    /// Anyone but the owner can read or write the file.
    #[error("dev profile {path} has mode {mode:o}; it must be 600")]
    Exposed { path: PathBuf, mode: u32 },
    /// The TOML is malformed or doesn't have the profile's fields.
    #[error(
        "dev profile {path}: {message}{}",
        .line.map(|line| format!(" (line {line})")).unwrap_or_default()
    )]
    Malformed {
        path: PathBuf,
        /// Where toml found the problem, when it points at a place.
        line: Option<usize>,
        message: String,
    },
    #[error("dev profile {path}: {field} is empty")]
    Empty { path: PathBuf, field: &'static str },
    /// The key prefix isn't a folder path coven can use.
    #[error("dev profile {path}: {field}: {error}")]
    Prefix {
        path: PathBuf,
        field: &'static str,
        #[source]
        error: coven::S3PrefixError,
    },
    /// The key prefix is only slashes, which is the bucket's root: the reset
    /// would empty the whole bucket.
    #[error(
        "dev profile {path}: {field} is the bucket's root; the reset would empty the whole bucket"
    )]
    WholeBucket { path: PathBuf, field: &'static str },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    library_name: String,
    identify_automatically: bool,
    import_when_identified: bool,
    s3: S3Section,
    discogs: DiscogsSection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct S3Section {
    endpoint: String,
    region: String,
    bucket: String,
    key_prefix: String,
    access_key_id: String,
    secret_access_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscogsSection {
    token: String,
}

impl DevResetProfile {
    /// Read the profile at `path`, which must be mode 600.
    pub fn read(path: &Path) -> Result<Self, DevResetProfileError> {
        let read_error = |error| DevResetProfileError::Read {
            path: path.to_path_buf(),
            error,
        };
        let mode = std::fs::metadata(path).map_err(read_error)?.permissions();
        let mode = unix_mode(&mode);
        if mode != 0o600 {
            return Err(DevResetProfileError::Exposed {
                path: path.to_path_buf(),
                mode,
            });
        }
        let text = std::fs::read_to_string(path).map_err(read_error)?;
        Self::parse(path, &text)
    }

    fn parse(path: &Path, text: &str) -> Result<Self, DevResetProfileError> {
        // toml's own message quotes the offending source line, which may be a
        // credential's, so only its description and line number are kept.
        let file: ProfileFile =
            toml::from_str(text).map_err(|error| DevResetProfileError::Malformed {
                path: path.to_path_buf(),
                line: error
                    .span()
                    .map(|span| text[..span.start].matches('\n').count() + 1),
                message: error.message().to_string(),
            })?;
        let required = |field: &'static str, value: String| {
            if value.trim().is_empty() {
                Err(DevResetProfileError::Empty {
                    path: path.to_path_buf(),
                    field,
                })
            } else {
                Ok(value)
            }
        };
        // The same parse storage setup uses: slashes at the prefix's ends
        // mean nothing, and what can't be a folder path fails here, before
        // the reset removes anything.
        const KEY_PREFIX: &str = "s3.key_prefix";
        let key_prefix = coven::S3Prefix::parse(&required(KEY_PREFIX, file.s3.key_prefix)?)
            .map_err(|error| DevResetProfileError::Prefix {
                path: path.to_path_buf(),
                field: KEY_PREFIX,
                error,
            })?;
        if key_prefix.is_empty() {
            return Err(DevResetProfileError::WholeBucket {
                path: path.to_path_buf(),
                field: KEY_PREFIX,
            });
        }
        // The refusal carries nothing beyond the field being empty.
        let library_name = LibraryName::parse(&file.library_name).map_err(|EmptyLibraryName| {
            DevResetProfileError::Empty {
                path: path.to_path_buf(),
                field: "library_name",
            }
        })?;
        Ok(Self {
            library_name,
            s3: S3ConfigData {
                bucket: required("s3.bucket", file.s3.bucket)?,
                region: required("s3.region", file.s3.region)?,
                endpoint: Some(required("s3.endpoint", file.s3.endpoint)?),
                key_prefix,
                access_key: required("s3.access_key_id", file.s3.access_key_id)?,
                secret_key: required("s3.secret_access_key", file.s3.secret_access_key)?,
                profile: None,
            },
            discogs_token: required("discogs.token", file.discogs.token)?,
            identify_automatically: file.identify_automatically,
            import_when_identified: file.import_when_identified,
        })
    }
}

/// Why the profile's storage prefix wasn't emptied.
#[derive(Debug, thiserror::Error)]
pub enum EmptyStoragePrefixError {
    #[error("dev profile s3.endpoint is not a URL: {0}")]
    Endpoint(#[source] url::ParseError),
    /// coven refused the location or a storage request failed; whatever it
    /// removed before then stays removed, and emptying again finishes.
    #[error(transparent)]
    Coven(#[from] coven::EmptyS3PrefixError),
}

/// Delete every object and abort every unfinished upload under the storage's
/// key prefix, whatever wrote them, so the library a reset creates there
/// starts on an empty remote. Storage at the bucket's root is refused: that
/// would empty the whole bucket.
pub async fn empty_storage_prefix(
    s3: S3ConfigData,
) -> Result<coven::EmptiedS3Prefix, EmptyStoragePrefixError> {
    let (location, credentials) = s3
        .into_location()
        .map_err(EmptyStoragePrefixError::Endpoint)?;
    Ok(coven::empty_s3_prefix(location, credentials).await?)
}

fn unix_mode(permissions: &std::fs::Permissions) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    permissions.mode() & 0o777
}

#[cfg(test)]
#[path = "dev_reset_tests.rs"]
mod tests;
