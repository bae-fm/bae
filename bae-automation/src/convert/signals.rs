//! What extraction read off a candidate — each signal, where each value was
//! read, and what the files say about the medium the audio was ripped from —
//! mirrored into the JSON shapes an MCP client reads.

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

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationTextOrigin = bae_core::signals::TextOrigin,
    from_core: pub(crate) fn,
    variants: { CueSheet, Artwork, FolderName, Filename, TextFile },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSignalOrigin = bae_core::signals::SignalOrigin,
    from_core: pub(crate) fn,
    variants: { Text(origin: (AutomationTextOrigin)), ArtworkBarcode },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationToolbarOrigin = bae_core::identify::ToolbarOrigin,
    from_core: pub(crate) fn,
    variants: { DiscToc, Value(origin: (AutomationSignalOrigin)) },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationImageRegion = bae_core::signals::ImageRegion,
    from_core: pub(crate) fn,
    fields: { x, y, width, height },
}

mirror_struct! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationSourcedValue = bae_core::signals::SourcedValue,
    from_core: pub(crate) fn,
    fields: {
        value,
        origin: (AutomationSignalOrigin),
        origin_path,
        region: (opt AutomationImageRegion),
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationBarcodeSignal = bae_core::signals::BarcodeSignal,
    from_core: pub(crate) fn,
    variants: {
        Scanning { codes: (each AutomationSourcedValue) },
        Settled { codes: (each AutomationSourcedValue) },
        Failed {
            failure: (AutomationLookupFailure),
            codes: (each AutomationSourcedValue),
        },
        Absent,
    },
}

mirror_enum! {
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    AutomationTextSignal = bae_core::signals::TextSignal,
    from_core: pub(crate) fn,
    variants: {
        Scanning { catalogs: (each AutomationSourcedValue), free_text },
        Settled { catalogs: (each AutomationSourcedValue), free_text },
        Failed {
            failure: (AutomationLookupFailure),
            catalogs: (each AutomationSourcedValue),
            free_text,
        },
    },
}

impl AutomationDiscIdSignal {
    /// Not a copy: core's `Computed` names the LOG or CUE the disc ID came
    /// from, which the automation shape does not carry.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    pub(crate) fn from_core(signal: bae_core::signals::DiscIdSignal) -> Self {
        use bae_core::signals::DiscIdSignal;
        match signal {
            DiscIdSignal::Computed {
                disc_id,
                track_count,
                ..
            } => Self::Computed {
                disc_id,
                track_count,
            },
            DiscIdSignal::Absent { track_count } => Self::Absent { track_count },
            DiscIdSignal::NotCdAudio {
                track_count,
                sample_rate_hz,
            } => Self::NotCdAudio {
                track_count,
                sample_rate_hz,
            },
            DiscIdSignal::Failed {
                failure,
                track_count,
            } => Self::Failed {
                failure: AutomationLookupFailure::from_core(failure),
                track_count,
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
        NotCd { sample_rate_hz },
        Unproven,
    },
}

impl AutomationSignals {
    /// Not a copy: core's `durations` are what the Ready rule narrows with, not
    /// a lookup input a client reads.
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
