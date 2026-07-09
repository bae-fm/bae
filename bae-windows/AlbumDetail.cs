using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>One album's detail plus every release with its tracks.</summary>
public sealed class AlbumDetail
{
    private readonly BridgeAlbumDetail _detail;

    internal AlbumDetail(BridgeAlbumDetail detail)
    {
        _detail = detail;
        Releases = detail.Releases.Select(release => new Release(release)).ToList();
    }

    public string Id => _detail.Album.Id;
    public string Title => _detail.Album.Title;
    public string Artist => _detail.Album.ArtistNames;
    public string PrimaryReleaseId => _detail.Album.PrimaryReleaseId;
    public List<Release> Releases { get; }
}

/// <summary>One release within an album's detail, shown in the release picker.</summary>
public sealed class Release
{
    private readonly BridgeRelease _release;

    internal Release(BridgeRelease release)
    {
        _release = release;
        Tracks = release.Tracks.Select(track => new Track(track)).ToList();
        Files = release.Files.Select(file => new ReleaseFile(file)).ToList();
    }

    public string ReleaseId => _release.Id;
    public string DisplayName => _release.DisplayName;
    public List<Track> Tracks { get; }
    public List<ReleaseFile> Files { get; }

    /// <summary>Whether this release lives in the cloud (Remote) rather than
    /// locally.</summary>
    public bool IsManaged => _release.StorageState == BridgeReleaseStorageState.Remote;

    /// <summary>Whether coven keeps this release's blobs pinned (kept offline) on
    /// this device.</summary>
    public bool Pinned => _release.Pinned;

    /// <summary>The storage transitions available right now, gated on cloud-home
    /// by the core; the album-detail storage band renders one button per entry.</summary>
    public IReadOnlyList<BridgeReleaseStorageAction> StorageActions => _release.StorageActions;

    /// <summary>The transition in flight for this release, or null when idle.</summary>
    public BridgeReleaseStorageAction? TransferAction => _release.TransferAction;

    /// <summary>The picker label.</summary>
    public override string ToString() => DisplayName;
}

/// <summary>One track row in an album's detail.</summary>
public sealed class Track
{
    private readonly BridgeTrack _track;

    internal Track(BridgeTrack track)
    {
        _track = track;
    }

    public string TrackId => _track.Id;
    public string Title => _track.Title;
    public long? DurationMs => _track.DurationMs;
    public string Artist => _track.ArtistNames;
    public string PositionLabel => _track.PositionText;
    public string DurationLabel => Loc.Duration(DurationMs);

    /// <summary>The list row; used as the default item text.</summary>
    public override string ToString() => $"{PositionLabel}  {Title}  {DurationLabel}".Trim();
}

/// <summary>One file in a release.</summary>
public sealed class ReleaseFile
{
    private readonly BridgeFile _file;

    internal ReleaseFile(BridgeFile file)
    {
        _file = file;
        AudioFormat = file.AudioFormat is null ? null : new AudioFormat(file.AudioFormat);
    }

    public string Id => _file.Id;
    public string OriginalFilename => _file.OriginalFilename;
    public long FileSize => _file.FileSize;
    public string ContentType => _file.ContentType;
    public bool IsImage => _file.IsImage;
    public AudioFormat? AudioFormat { get; }
    public string SizeLabel => Loc.Bytes(FileSize);
}

/// <summary>A file's structured audio format.</summary>
public sealed class AudioFormat
{
    private readonly BridgeAudioFormat _format;

    internal AudioFormat(BridgeAudioFormat format)
    {
        _format = format;
    }

    public string Codec => _format.Codec;
    public long SampleRateHz => _format.SampleRateHz;
    public long? BitsPerSample => _format.BitsPerSample;
    public long? BitrateKbps => _format.BitrateKbps;
    public long Channels => _format.Channels;

    /// <summary>The one-line descriptor, e.g. codec, rate, bit depth, channels.</summary>
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

    private string ChannelsText
    {
        get
        {
            var key = NativeBae.AudioChannelsKey(Channels);
            return key is null ? $"{Loc.Number(Channels)}ch" : Loc.Core(key);
        }
    }
}
