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

/// The name of a preset called `name` once its format changes from `from` to
/// `to`, with `taken` the names of the other formats.
#[uniffi::export]
pub fn bridge_save_name_after_format_change(
    name: String,
    from: BridgeSaveFormat,
    to: BridgeSaveFormat,
    taken: Vec<String>,
) -> String {
    let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
    bae_preferences::SaveFormat::name_after_change(&name, from.into_core(), to.into_core(), &taken)
}

/// Whether a format of `presets` can be deleted: the only one left cannot.
#[uniffi::export]
pub fn bridge_save_preset_can_delete(presets: Vec<BridgeSavePreset>) -> bool {
    let presets: Vec<_> = presets.into_iter().map(BridgeSavePreset::into_core).collect();
    bae_preferences::SavePreset::can_delete(&presets)
}

/// Whether a format other than `own_id` among `presets` is already named
/// `name`.
#[uniffi::export]
pub fn bridge_save_name_taken(
    name: String,
    own_id: String,
    presets: Vec<BridgeSavePreset>,
) -> bool {
    let presets: Vec<_> = presets.into_iter().map(BridgeSavePreset::into_core).collect();
    bae_preferences::SavePreset::name_taken(&name, &own_id, &presets)
}

/// The extension of the files `codec` writes, without its dot.
#[uniffi::export]
pub fn bridge_save_codec_extension(codec: &BridgeSaveCodec) -> String {
    codec.clone().into_core().extension().to_string()
}

/// The format `codec` saves as.
#[uniffi::export]
pub fn bridge_save_codec_format(codec: &BridgeSaveCodec) -> BridgeSaveFormat {
    BridgeSaveFormat::from_core(codec.clone().into_core().format())
}

#[cfg(test)]
#[path = "save_format_tests.rs"]
mod tests;
