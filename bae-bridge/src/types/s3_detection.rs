use super::*;

#[derive(uniffi::Object)]
pub struct BridgeS3Detection {
    detection: coven::S3Detection,
}
impl BridgeS3Detection {
    pub(crate) fn from_core(detection: coven::S3Detection) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self { detection })
    }
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeS3Versioning {
    Known {
        state: BridgeS3BucketVersioning,
    },
    Failed {
        failure: BridgeStorageFailure,
        detail: String,
    },
}
#[derive(Clone, uniffi::Enum)]
pub enum BridgeS3Probe {
    Signing { signing: BridgeS3Signing },
    SinglePut { condition: BridgeS3SinglePut },
    PartialUploadHidden,
    ConditionalCompletion,
    CompletionTimestamp,
    CopyPublication { condition: BridgeS3CopyCondition },
    CopyTimestamp,
    Listing { listing: BridgeS3Listing },
    ChecksumMode,
    PassiveChecksum,
    ChecksumReturned,
    ChecksumScopeStated,
    GoogleGeneration,
    GoogleHeaders,
    BucketList,
}
mirror_enum! {BridgeS3Probe=coven::S3Probe,from_core:fn,variants:{
    Signing(signing:(BridgeS3Signing)),SinglePut(condition:(BridgeS3SinglePut)),PartialUploadHidden,
    ConditionalCompletion,CompletionTimestamp,CopyPublication(condition:(BridgeS3CopyCondition)),CopyTimestamp,
    Listing(listing:(BridgeS3Listing)),ChecksumMode,PassiveChecksum,ChecksumReturned,ChecksumScopeStated,GoogleGeneration,GoogleHeaders,BucketList,
},}
#[derive(Clone, uniffi::Record)]
pub struct BridgeS3ProbeResult {
    pub probe: BridgeS3Probe,
    pub held: bool,
    pub detail: String,
}
#[uniffi::export]
impl BridgeS3Detection {
    pub fn settings(&self) -> BridgeS3Settings {
        BridgeS3Settings::from_core(self.detection.settings)
    }
    pub fn versioning(&self) -> BridgeS3Versioning {
        match &self.detection.versioning {
            Ok(state) => BridgeS3Versioning::Known {
                state: BridgeS3BucketVersioning::from_core(*state),
            },
            Err(error) => BridgeS3Versioning::Failed {
                failure: BridgeStorageFailure::from_core(error.failure()),
                detail: error.to_string(),
            },
        }
    }
    pub fn results(&self) -> Vec<BridgeS3ProbeResult> {
        self.detection
            .results
            .iter()
            .map(|result| BridgeS3ProbeResult {
                probe: BridgeS3Probe::from_core(result.probe),
                held: result.held,
                detail: result.detail.clone(),
            })
            .collect()
    }
    pub fn check(&self, settings: BridgeS3Settings) -> Result<(), BridgeError> {
        self.detection
            .check(&settings.into_core())
            .map_err(|error| BridgeError::from(bae_core::library::LibraryError::from(error)))
    }
}
