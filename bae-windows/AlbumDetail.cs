using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Bae.Windows;

/// <summary>
/// One album's detail, deserialized from the FFI's <c>bae_album_detail</c> JSON.
/// Header fields plus every release with its tracks; the view shows
/// <see cref="PrimaryReleaseId"/> first and lets the user switch releases.
/// </summary>
public sealed class AlbumDetail
{
    public string Id { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;
    public string PrimaryReleaseId { get; set; } = string.Empty;
    public List<Release> Releases { get; set; } = new();
}

/// <summary>One release within an album's detail, shown in the release picker.</summary>
public sealed class Release
{
    public string ReleaseId { get; set; } = string.Empty;
    public string DisplayName { get; set; } = string.Empty;
    public List<Track> Tracks { get; set; } = new();

    /// <summary>The release's files (audio + images), each carrying its
    /// structured audio format where applicable.</summary>
    public List<ReleaseFile> Files { get; set; } = new();

    /// <summary>The picker label.</summary>
    public override string ToString() => DisplayName;
}

/// <summary>One track row in an album's detail.</summary>
public sealed class Track
{
    public string TrackId { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;

    /// <summary>Structured position; the locale never crosses the bridge, so the
    /// position string ("A1"/"2-3"/"5") is composed here from the case.</summary>
    public TrackPosition Position { get; set; } = new();

    /// <summary>Raw track length in milliseconds, or null when unknown.</summary>
    public long? DurationMs { get; set; }

    public string Artist { get; set; } = string.Empty;

    /// <summary>The composed position string, e.g. "A1" / "2-3" / "5".</summary>
    public string PositionLabel => Position.Label;

    /// <summary>The track length formatted for the locale, e.g. "3:07"; empty if
    /// unknown.</summary>
    public string DurationLabel => Loc.Duration(DurationMs);

    /// <summary>The list row; used as the default item text.</summary>
    public override string ToString() => $"{PositionLabel}  {Title}  {DurationLabel}".Trim();
}

/// <summary>
/// A track's structured display position, mirroring the FFI's
/// <c>FfiTrackPosition</c> (and the bridge's <c>BridgeTrackPosition</c>).
/// <see cref="Kind"/> tags the case; only that case's fields are set. The UI
/// composes the position string mechanically — no prose crosses the bridge.
/// </summary>
public sealed class TrackPosition
{
    /// <summary>"sided" / "disc" / "flat".</summary>
    public string Kind { get; set; } = "flat";

    /// <summary>Side letter (A/B/C…) for the "sided" case.</summary>
    public string? SideLetter { get; set; }

    /// <summary>Disc number for the "disc" case.</summary>
    public int Disc { get; set; }

    /// <summary>Within-side / within-disc / flat track number.</summary>
    public int Number { get; set; }

    /// <summary>The composed position string: "{side_letter}{number}" (A1),
    /// "{disc}-{number}" (2-3), or "{number}" (5).</summary>
    [JsonIgnore]
    public string Label => Kind switch
    {
        "sided" => $"{SideLetter}{Number}",
        "disc" => $"{Disc}-{Number}",
        _ => Number.ToString(System.Globalization.CultureInfo.CurrentCulture),
    };
}

/// <summary>
/// One file in a release, mirroring the FFI's <c>FfiFile</c> (and the bridge's
/// <c>BridgeFile</c>). <see cref="AudioFormat"/> is null for non-audio files.
/// </summary>
public sealed class ReleaseFile
{
    public string Id { get; set; } = string.Empty;
    public string OriginalFilename { get; set; } = string.Empty;
    public long FileSize { get; set; }
    public string ContentType { get; set; } = string.Empty;
    public bool IsImage { get; set; }

    /// <summary>Structured audio format for an audio file; null otherwise.</summary>
    public AudioFormat? AudioFormat { get; set; }

    /// <summary>File size formatted for the locale, e.g. "12.4 MB".</summary>
    public string SizeLabel => Loc.Bytes(FileSize);
}

/// <summary>
/// A file's structured audio format, mirroring the FFI's <c>FfiAudioFormat</c>
/// (and the bridge's <c>BridgeAudioFormat</c>). The one-line descriptor is
/// composed here from the parts: the codec is a proper noun, numbers format per
/// locale, and the channel count maps to a localized word.
/// <see cref="BitsPerSample"/> present means lossless (show the bit depth);
/// absent means lossy (show <see cref="BitrateKbps"/>).
/// </summary>
public sealed class AudioFormat
{
    public string Codec { get; set; } = string.Empty;
    public long SampleRateHz { get; set; }
    public long? BitsPerSample { get; set; }
    public long? BitrateKbps { get; set; }
    public long Channels { get; set; }

    /// <summary>The one-line descriptor, e.g. "FLAC · 44.1 kHz · 16-bit · stereo"
    /// (lossless) or "MP3 · 320 kbps · 44.1 kHz · stereo" (lossy).</summary>
    [JsonIgnore]
    public string Text
    {
        get
        {
            var culture = System.Globalization.CultureInfo.CurrentCulture;
            var parts = new List<string> { Codec };
            if (BitsPerSample is null && BitrateKbps is { } kbps)
            {
                parts.Add($"{Loc.Number(kbps)} kbps");
            }
            var khz = SampleRateHz / 1000.0;
            parts.Add($"{khz.ToString("0.#", culture)} kHz");
            if (BitsPerSample is { } bits)
            {
                parts.Add($"{Loc.Number(bits)}-bit");
            }
            parts.Add(ChannelsText);
            return string.Join(" · ", parts);
        }
    }

    /// <summary>The localized channel word ("mono"/"stereo") or "{n}ch" for
    /// counts with no word. The key comes from the FFI (one source for the
    /// mapping), resolved through the Core catalog.</summary>
    private string ChannelsText
    {
        get
        {
            var key = NativeBae.AudioChannelsKey(Channels);
            return key is null ? $"{Loc.Number(Channels)}ch" : Loc.Core(key);
        }
    }
}
