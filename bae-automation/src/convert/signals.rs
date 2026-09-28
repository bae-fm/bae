//! What extraction read off a candidate — each signal, and what the files say
//! about the medium the audio was ripped from — mirrored into the JSON shapes
//! an MCP client reads.

use super::*;

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationLookupFailure = bae_core::signals::LookupFailure,
    from_core: pub(crate) fn,
    variants: {
        Network,
        Provider { status },
        Timeout,
        ArtworkAnalysis,
        Diagnostic { detail },
    },
}

impl AutomationBarcodeSignal {
    /// Not a copy: core keeps a code once per file it was read off, and this
    /// lists each code once.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signal: bae_core::signals::BarcodeSignal) -> Self {
        use bae_core::signals::{BarcodeSignal, SourcedValue};
        match signal {
            BarcodeSignal::Scanning { codes } => Self::Scanning {
                codes: SourcedValue::values(&codes),
            },
            BarcodeSignal::Settled { codes } => Self::Settled {
                codes: SourcedValue::values(&codes),
            },
            BarcodeSignal::Failed { failure, codes } => Self::Failed {
                failure: AutomationLookupFailure::from_core(failure),
                codes: SourcedValue::values(&codes),
            },
            BarcodeSignal::Absent => Self::Absent,
        }
    }
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationTextSignal = bae_core::signals::TextSignal,
    from_core: pub(crate) fn,
    variants: {
        Scanning { catalogs, free_text },
        Settled { catalogs, free_text },
        Failed { failure: (AutomationLookupFailure), catalogs, free_text },
    },
}

impl AutomationDiscIdSignal {
    /// Not a copy: the file a computed disc ID came from does not cross.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signal: bae_core::signals::DiscIdSignal) -> Self {
        use bae_core::signals::DiscIdSignal;
        match signal {
            DiscIdSignal::Computed { disc_id, .. } => Self::Computed { disc_id },
            DiscIdSignal::Absent => Self::Absent,
            DiscIdSignal::NotCdAudio => Self::NotCdAudio,
            DiscIdSignal::Failed { failure } => Self::Failed {
                failure: AutomationLookupFailure::from_core(failure),
            },
        }
    }
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationCdProof = bae_core::signals::CdProof,
    from_core: pub(crate) fn,
    variants: { RipLog, AccurateRipReport, RipperSheet },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationStoreMarker = bae_core::signals::StoreMarker,
    from_core: pub(crate) fn,
    variants: { ITunesPurchase, Bandcamp },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationDownloadProof = bae_core::signals::DownloadProof,
    from_core: pub(crate) fn,
    variants: {
        Store { marker: (AutomationStoreMarker), file },
        DeliverySet,
    },
}

impl AutomationAudioSource {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(source: bae_core::signals::AudioSource) -> Self {
        match source {
            bae_core::signals::AudioSource::CdRip { proof, file } => Self::CdRip {
                proof: AutomationCdProof::from_core(proof),
                file,
            },
            bae_core::signals::AudioSource::Download(proof) => Self::Download {
                proof: AutomationDownloadProof::from_core(proof),
            },
        }
    }
}

impl AutomationAudioOrigin {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(origin: bae_core::signals::AudioOrigin) -> Self {
        Self {
            source: origin.source.map(AutomationAudioSource::from_core),
            not_cd_rate: origin.not_cd_rate,
        }
    }
}

impl AutomationSignals {
    /// Not a copy: the text pool does not cross.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signals: bae_core::signals::Signals) -> Self {
        Self {
            origin: AutomationAudioOrigin::from_core(signals.origin),
            disc_id: AutomationDiscIdSignal::from_core(signals.disc_id),
            barcode: AutomationBarcodeSignal::from_core(signals.barcode),
            text: AutomationTextSignal::from_core(signals.text),
            isrcs: signals.isrcs,
        }
    }
}
