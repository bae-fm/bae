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

/// Every format a preset can save as, in the order a format picker offers
/// them, in the groups it sets apart: the lossless formats, then the lossy
/// ones.
#[uniffi::export]
pub fn bridge_save_format_groups() -> Vec<Vec<BridgeSaveFormat>> {
    bae_preferences::SaveFormat::groups()
        .into_iter()
        .map(|group| group.into_iter().map(BridgeSaveFormat::from_core).collect())
        .collect()
}

/// The codec a preset of `format` starts at.
#[uniffi::export]
pub fn bridge_save_format_default_codec(format: BridgeSaveFormat) -> BridgeSaveCodec {
    BridgeSaveCodec::from_core(&format.into_core().default_codec())
}

/// The bitrates a lossy `format` saves at, lowest first; `None` for a
/// lossless one, which has a bit depth instead.
#[uniffi::export]
pub fn bridge_save_format_bitrates_kbps(format: BridgeSaveFormat) -> Option<Vec<u32>> {
    format.into_core().bitrates_kbps().map(<[u32]>::to_vec)
}

/// `codec` carried over to `format`, keeping the setting that still applies.
#[uniffi::export]
pub fn bridge_save_codec_switched_to(
    codec: &BridgeSaveCodec,
    format: BridgeSaveFormat,
) -> BridgeSaveCodec {
    BridgeSaveCodec::from_core(&codec.clone().into_core().switched_to(format.into_core()))
}

/// The name of `format`, a proper noun shown as-is in every locale.
#[uniffi::export]
pub fn bridge_save_format_name(format: BridgeSaveFormat) -> String {
    format.into_core().name().to_string()
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
