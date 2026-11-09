use super::super::*;

/// A format a save preset saves as: core's `SaveFormat`. A preset's codec is
/// one format at its own settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum BridgeSaveFormat {
    Flac,
    Mp3,
    Aac,
    OpusOgg,
    Wav,
    Aiff,
}

mirror_enum! {
    BridgeSaveFormat = bae_preferences::SaveFormat,
    from_core: pub(crate) fn,
    into_core: pub(crate) fn,
    variants: { Flac, Mp3, Aac, OpusOgg, Wav, Aiff },
}

/// The bitrates a lossy format saves at, in kbps, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct BridgeKbpsRange {
    pub min: u32,
    pub max: u32,
}

/// Every format a preset can save as, in the order a format picker offers
/// them.
#[uniffi::export]
pub fn bridge_save_formats() -> Vec<BridgeSaveFormat> {
    bae_preferences::SaveFormat::ALL.into_iter().map(BridgeSaveFormat::from_core).collect()
}

/// The codec a preset of `format` starts at.
#[uniffi::export]
pub fn bridge_save_format_default_codec(format: BridgeSaveFormat) -> BridgeSaveCodec {
    BridgeSaveCodec::from_core(&format.into_core().default_codec())
}

/// The bitrates a lossy `format` saves at; `None` for a lossless one, which
/// has a bit depth instead.
#[uniffi::export]
pub fn bridge_save_format_bitrates_kbps(format: BridgeSaveFormat) -> Option<BridgeKbpsRange> {
    format
        .into_core()
        .bitrates_kbps()
        .map(|range| BridgeKbpsRange { min: *range.start(), max: *range.end() })
}

/// The name of `format`, a proper noun shown as-is in every locale.
#[uniffi::export]
pub fn bridge_save_format_name(format: BridgeSaveFormat) -> String {
    format.into_core().name().to_string()
}

/// The preset a person adds to their list, as core makes it: FLAC, the
/// default filename pattern and pregap placement, both levels, the cover
/// embedded, under a new id.
#[uniffi::export]
pub fn bridge_new_save_preset() -> BridgeSavePreset {
    let ids =
        bae_util::time_ordered_ids::TimeOrderedIds::new(std::sync::Arc::new(coven::SystemClock));
    BridgeSavePreset::from_core(&bae_preferences::SavePreset::new_flac(&ids))
}

/// The extension of the files `codec` writes, without its dot.
#[uniffi::export]
pub fn bridge_save_codec_extension(codec: &BridgeSaveCodec) -> String {
    codec.clone().into_core().extension().to_string()
}

/// Whether `codec` can write a release as one file with a CUE sheet.
#[uniffi::export]
pub fn bridge_save_codec_supports_single_file_cue(codec: &BridgeSaveCodec) -> bool {
    codec.clone().into_core().supports_single_file_cue()
}

/// The format `codec` saves as.
#[uniffi::export]
pub fn bridge_save_codec_format(codec: &BridgeSaveCodec) -> BridgeSaveFormat {
    BridgeSaveFormat::from_core(codec.clone().into_core().format())
}

#[cfg(test)]
#[path = "save_format_tests.rs"]
mod tests;
