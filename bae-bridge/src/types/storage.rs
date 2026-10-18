use super::BridgeError;
use super::BridgeStoreKeyState;

/// Provider location without credentials, as recorded by coven.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageLocation {
    S3 {
        bucket: String,
        region: String,
        endpoint: Option<String>,
        profile: Option<BridgeS3Profile>,
        prefix: String,
    },
    Dropbox {
        location: BridgeDropboxLocation,
    },
    OneDrive {
        drive_id: String,
        folder_id: String,
    },
    CloudKit {
        container: String,
        owner: String,
        zone: String,
    },
}

impl BridgeStorageLocation {
    pub(crate) fn from_core(value: coven::StorageConfig) -> Self {
        match value {
            coven::StorageConfig::S3 {
                bucket,
                region,
                endpoint,
                profile,
                prefix,
            } => Self::S3 {
                bucket,
                region,
                endpoint: endpoint.map(Into::into),
                profile: profile.map(BridgeS3Profile::from_core),
                prefix,
            },
            coven::StorageConfig::Dropbox { location } => Self::Dropbox {
                location: BridgeDropboxLocation::from_core(location),
            },
            coven::StorageConfig::OneDrive {
                drive_id,
                folder_id,
            } => Self::OneDrive {
                drive_id,
                folder_id,
            },
            coven::StorageConfig::CloudKit {
                container,
                owner,
                zone,
            } => Self::CloudKit {
                container,
                owner,
                zone,
            },
        }
    }

    pub(crate) fn into_core(self) -> Result<coven::StorageConfig, BridgeError> {
        Ok(match self {
            Self::S3 {
                bucket,
                region,
                endpoint,
                profile,
                prefix,
            } => coven::StorageConfig::S3 {
                bucket,
                region,
                endpoint: endpoint
                    .map(|s| s.parse())
                    .transpose()
                    .map_err(BridgeError::config)?,
                profile: profile.map(BridgeS3Profile::into_core),
                prefix,
            },
            Self::Dropbox { location } => coven::StorageConfig::Dropbox {
                location: location.into_core(),
            },
            Self::OneDrive {
                drive_id,
                folder_id,
            } => coven::StorageConfig::OneDrive {
                drive_id,
                folder_id,
            },
            Self::CloudKit {
                container,
                owner,
                zone,
            } => coven::StorageConfig::CloudKit {
                container,
                owner,
                zone,
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeDropboxLocation {
    AppFolder,
    SharedFolder { namespace_id: String },
}
mirror_enum! {
    BridgeDropboxLocation = coven::DropboxLocation,
    from_core: fn,
        into_core: fn,
    variants: { AppFolder, SharedFolder { namespace_id } },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3Profile {
    Named { profile: BridgeS3ProfileName },
    Custom { settings: BridgeS3Settings },
}
mirror_enum! {
    BridgeS3Profile = coven::S3Profile,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Named(profile: (BridgeS3ProfileName)), Custom(settings: (BridgeS3Settings)) },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BridgeS3Settings {
    pub signing: BridgeS3Signing,
    pub single_put: BridgeS3SinglePut,
    pub copy_publication: BridgeS3CopyPublication,
    pub multipart_timestamp: BridgeS3MultipartTimestamp,
    pub checksums: BridgeS3Checksums,
    pub listing: BridgeS3Listing,
    pub revision: BridgeS3Revision,
    pub response_headers: BridgeS3ResponseHeaders,
    pub request_checksums: bool,
}
mirror_struct! {
    BridgeS3Settings = coven::S3Settings,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    fields: {
        signing: (BridgeS3Signing), single_put: (BridgeS3SinglePut),
        copy_publication: (BridgeS3CopyPublication), multipart_timestamp: (BridgeS3MultipartTimestamp),
        checksums: (BridgeS3Checksums), listing: (BridgeS3Listing), revision: (BridgeS3Revision),
        response_headers: (BridgeS3ResponseHeaders), request_checksums,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3CopyPublication {
    Never,
    Multipart { condition: BridgeS3CopyCondition },
    Every { condition: BridgeS3CopyCondition },
}
mirror_enum! {
    BridgeS3CopyPublication = coven::S3CopyPublication,
    from_core: fn, into_core: fn,
    variants: { Never, Multipart(condition: (BridgeS3CopyCondition)), Every(condition: (BridgeS3CopyCondition)) },
}

#[derive(Clone, uniffi::Record)]
pub struct BridgeStorageSetup {
    pub location: BridgeStorageLocation,
    pub key_state: BridgeStoreKeyState,
    pub restore_code: String,
}
impl BridgeStorageSetup {
    pub(crate) fn from_core(value: coven::StorageSetup) -> Self {
        Self {
            location: BridgeStorageLocation::from_core(value.connection.storage),
            key_state: value.connection.key_state.into(),
            restore_code: value.restore_code.as_str().to_string(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BridgeStorageFolder {
    pub name: String,
    pub location: BridgeStorageLocation,
}
#[cfg(feature = "oauth-providers")]
impl BridgeStorageFolder {
    pub(crate) fn from_core(value: coven::StorageFolder) -> Self {
        Self {
            name: value.name,
            location: BridgeStorageLocation::from_core(value.storage),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3ProfileName {
    Aws,
    Gcs,
    R2,
    RustFs,
    SeaweedFs,
    CephRgw,
    VersityGw,
}
mirror_enum! {
    BridgeS3ProfileName = coven::S3ProfileName,
    from_core: fn, into_core: fn,
    variants: { Aws, Gcs, R2, RustFs, SeaweedFs, CephRgw, VersityGw },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3Signing {
    AwsSigV4,
    GoogleHmac,
}
mirror_enum! {
    BridgeS3Signing = coven::S3Signing,
    from_core: pub(crate) fn, into_core: fn,
    variants: { AwsSigV4, GoogleHmac },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3SinglePut {
    IfNoneMatch,
    GenerationMatch,
}
mirror_enum! {
    BridgeS3SinglePut = coven::S3SinglePut,
    from_core: pub(crate) fn, into_core: fn,
    variants: { IfNoneMatch, GenerationMatch },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3CopyCondition {
    IfNoneMatch,
    GenerationMatch,
    CloudflareIfNoneMatch,
}
mirror_enum! {
    BridgeS3CopyCondition = coven::S3CopyCondition,
    from_core: pub(crate) fn, into_core: fn,
    variants: { IfNoneMatch, GenerationMatch, CloudflareIfNoneMatch },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3MultipartTimestamp {
    Completion,
    Start,
}
mirror_enum! {
    BridgeS3MultipartTimestamp = coven::S3MultipartTimestamp,
    from_core: fn, into_core: fn,
    variants: { Completion, Start },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3Checksums {
    ScopeStated,
    UnstatedIgnored,
    Absent,
}
mirror_enum! {
    BridgeS3Checksums = coven::S3Checksums,
    from_core: fn, into_core: fn,
    variants: { ScopeStated, UnstatedIgnored, Absent },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3Listing {
    V2,
    V1,
}
mirror_enum! {
    BridgeS3Listing = coven::S3Listing,
    from_core: pub(crate) fn, into_core: fn,
    variants: { V2, V1 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3Revision {
    ETag,
    Generation,
}
mirror_enum! {
    BridgeS3Revision = coven::S3Revision,
    from_core: fn, into_core: fn,
    variants: { ETag, Generation },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3ResponseHeaders {
    S3,
    Google,
}
mirror_enum! {
    BridgeS3ResponseHeaders = coven::S3ResponseHeaders,
    from_core: fn, into_core: fn,
    variants: { S3, Google },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeS3BucketVersioning {
    Disabled,
    Enabled,
    Suspended,
    Unavailable,
}
mirror_enum! {BridgeS3BucketVersioning=coven::S3BucketVersioning,from_core:pub(crate) fn,variants:{Disabled,Enabled,Suspended,Unavailable},}
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum BridgeStorageCheck {
    Create,
    CreateOnce,
    ConditionalReplace,
    Read,
    ReadRange,
    List,
    Delete,
    MultipartCreateOnce,
    PublicationTime,
    Behavior,
}
mirror_enum! {BridgeStorageCheck=coven::StorageCheck,from_core:pub(crate) fn,variants:{Create,CreateOnce,ConditionalReplace,Read,ReadRange,List,Delete,MultipartCreateOnce,PublicationTime,Behavior},}
