using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Stateless codec helpers: the family the Format picker offers, the labels, the
// codec/bit-depth/bitrate/pregap choice sets, and the codec-switch mapping. Split
// out of FormatsSettingsSection.cs unchanged.
internal sealed partial class FormatsSettingsSection
{
    // The codec families the Format picker and the add-preset flyout offer.
    // Format names are proper nouns, shown as-is in every locale.
    private static readonly string[] PresetKinds = { "flac", "mp3", "aac", "opus_ogg", "wav", "aiff" };

    private static string KindLabel(string kind) => kind switch
    {
        "mp3" => "MP3",
        "aac" => "AAC",
        "opus_ogg" => "Opus",
        "wav" => "WAV",
        "aiff" => "AIFF",
        _ => "FLAC",
    };

    private static string CodecKind(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 => "mp3",
        BridgeSaveCodec.Aac => "aac",
        BridgeSaveCodec.OpusOgg => "opus_ogg",
        BridgeSaveCodec.Wav => "wav",
        BridgeSaveCodec.Aiff => "aiff",
        _ => "flac",
    };

    // Switch codec family, carrying the parameter that still applies: bit depth
    // across lossless codecs, bitrate across lossy ones (clamped into the target
    // family's supported range). A cross-family switch takes the family default.
    private static BridgeSaveCodec SwitchedCodec(BridgeSaveCodec codec, string kind)
    {
        var bitDepth = LosslessBitDepth(codec) ?? BridgeSaveBitDepth.Source;
        var bitrate = LossyBitrate(codec);
        return kind switch
        {
            "mp3" => new BridgeSaveCodec.Mp3(Math.Clamp(bitrate ?? 320, 32, 320)),
            "aac" => new BridgeSaveCodec.Aac(Math.Clamp(bitrate ?? 256, 32, 512)),
            "opus_ogg" => new BridgeSaveCodec.OpusOgg(bitrate ?? 192),
            "wav" => new BridgeSaveCodec.Wav(bitDepth),
            "aiff" => new BridgeSaveCodec.Aiff(bitDepth),
            _ => new BridgeSaveCodec.Flac(bitDepth),
        };
    }

    // The lossless family's bit depth; null for lossy codecs.
    private static BridgeSaveBitDepth? LosslessBitDepth(BridgeSaveCodec codec) =>
        codec switch
        {
            BridgeSaveCodec.Flac flac => flac.BitDepth,
            BridgeSaveCodec.Wav wav => wav.BitDepth,
            BridgeSaveCodec.Aiff aiff => aiff.BitDepth,
            _ => null,
        };

    // The lossy family's bitrate; null for lossless codecs.
    private static uint? LossyBitrate(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 mp3 => mp3.BitrateKbps,
        BridgeSaveCodec.Aac aac => aac.BitrateKbps,
        BridgeSaveCodec.OpusOgg opus => opus.BitrateKbps,
        _ => null,
    };

    // The bitrate range core's preset validation accepts for the lossy families;
    // null for lossless, which carry no bitrate.
    private static (uint Min, uint Max)? BitrateRange(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 => (32u, 320u),
        BridgeSaveCodec.Aac => (32u, 512u),
        BridgeSaveCodec.OpusOgg => (32u, 512u),
        _ => null,
    };

    private static string CodecLabel(BridgeSaveCodec codec) => codec switch
    {
        BridgeSaveCodec.Mp3 mp3 =>
            Loc.Chrome("settings.formats.codec_mp3", "kbps", mp3.BitrateKbps),
        BridgeSaveCodec.Aac aac =>
            Loc.Chrome("settings.formats.codec_aac", "kbps", aac.BitrateKbps),
        BridgeSaveCodec.OpusOgg opus =>
            Loc.Chrome("settings.formats.codec_opus", "kbps", opus.BitrateKbps),
        BridgeSaveCodec.Wav => "WAV",
        BridgeSaveCodec.Aiff => "AIFF",
        _ => "FLAC",
    };

    private static List<(string Label, BridgeSaveBitDepth Value)> BitDepthChoices() => new()
    {
        (Loc.Chrome("settings.formats.bit_depth.source"), BridgeSaveBitDepth.Source),
        (Loc.Chrome("settings.formats.bit_depth.bits16"), BridgeSaveBitDepth.Bits16),
        (Loc.Chrome("settings.formats.bit_depth.bits24"), BridgeSaveBitDepth.Bits24),
        (Loc.Chrome("settings.formats.bit_depth.bits32"), BridgeSaveBitDepth.Bits32),
    };

    private static List<(string Label, BridgeSavePregapPlacement Value)> PregapChoices(BridgeSaveCodec codec)
    {
        var choices = new List<(string Label, BridgeSavePregapPlacement Value)>
        {
            (
                Loc.Chrome("settings.formats.pregap.append_except_htoa"),
                BridgeSavePregapPlacement.AppendToPreviousExceptHtoa
            ),
            (
                Loc.Chrome("settings.formats.pregap.append_including_htoa"),
                BridgeSavePregapPlacement.AppendToPreviousIncludingHtoa
            ),
            (Loc.Chrome("settings.formats.pregap.exclude"), BridgeSavePregapPlacement.Exclude),
        };
        if (codec is not (BridgeSaveCodec.OpusOgg or BridgeSaveCodec.Aac))
        {
            choices.Add((
                Loc.Chrome("settings.formats.pregap.single_file_with_cue"),
                BridgeSavePregapPlacement.SingleFileWithCue
            ));
        }
        return choices;
    }
}
