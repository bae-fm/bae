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
    AutomationRipEvidence = bae_core::signals::RipEvidence,
    from_core: pub(crate) fn,
    variants: {
        Cd { proof: (AutomationCdProof), file },
        NotCd,
        Unproven,
    },
}

impl AutomationSignals {
    /// Not a copy: the text pool does not cross.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signals: bae_core::signals::Signals) -> Self {
        Self {
            rip: AutomationRipEvidence::from_core(signals.rip),
            disc_id: AutomationDiscIdSignal::from_core(signals.disc_id),
            barcode: AutomationBarcodeSignal::from_core(signals.barcode),
            text: AutomationTextSignal::from_core(signals.text),
        }
    }
}
