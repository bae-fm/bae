using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using Avalonia.Threading;
using Avalonia.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Desktop;

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
public sealed class Release : INotifyPropertyChanged
{
    private readonly BridgeRelease _release;
    private readonly CoverImage.Binding _cover;

    internal Release(BridgeRelease release)
    {
        _release = release;
        Tracks = release.Tracks.Select(track => new Track(track)).ToList();
        _cover = new CoverImage.Binding(release.Cover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string ReleaseId => _release.Id;
    public string DisplayName => _release.DisplayName;
    public List<Track> Tracks { get; }

    /// <summary>Attach the current handle so the release's own cover loads off the
    /// UI thread and applies when it lands — the same (id, version) cache path the
    /// grid tiles use.</summary>
    internal void AttachCover(MediaPathsService mediaPaths, Dispatcher dispatcherQueue) =>
        _cover.Attach(mediaPaths, dispatcherQueue);

    /// <summary>The release's own cover, or null while it loads / when the release
    /// has none. The album expansion's large art binds here per selected release.</summary>
    public Bitmap? Cover => _cover.Source;

    /// <summary>Whether this release carries its own cover reference. When it
    /// doesn't, the expansion falls back to the album card's cover rather than
    /// showing blank art.</summary>
    internal bool HasOwnCover => _release.Cover is not null;

    /// <summary>Total playing time across the release's tracks, in the words core
    /// chose for it ("39 min", "3 hr, 42 min"); empty when no track reports a
    /// length.</summary>
    public string TotalDurationLabel => BridgeDisplay.DurationUnits(_release.TotalDuration);

    /// <summary>Whether this release lives in the cloud (Remote) rather than
    /// locally.</summary>
    public bool IsManaged => _release.StorageState == BridgeReleaseStorageState.Remote;

    /// <summary>Whether coven keeps this release's blobs pinned (kept offline) on
    /// this device.</summary>
    public bool Pinned => _release.Pinned;

    /// <summary>The storage transitions available right now, gated on cloud-home
    /// by the core; the album-detail storage band renders one button per entry.
    /// Internal: the generated bridge types are internal, so a public member
    /// exposing one is inconsistent accessibility (CS0053); every consumer is
    /// in-assembly.</summary>
    internal IReadOnlyList<BridgeReleaseStorageAction> StorageActions => _release.StorageActions;

    /// <summary>The transition in flight for this release, or null when idle.
    /// Internal for the same reason as <see cref="StorageActions"/>.</summary>
    internal BridgeReleaseStorageAction? TransferAction => _release.TransferAction;

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
    public string Artist => _track.ArtistNames;

    /// <summary>The artist to show on the row, or null for none — core's decision
    /// (set only for a compilation, where the album header names no single one).
    /// Windows dropped this entirely before; the row now shows what the other
    /// platforms show.</summary>
    public string? DisplayArtist => _track.DisplayArtist;

    public string PositionLabel => _track.PositionText;
    public string DurationLabel => BridgeDisplay.Clock(_track.DurationClock);

    /// <summary>The list row; used as the default item text. The display artist
    /// sits between the title and the duration when core provides one.</summary>
    public override string ToString()
    {
        var artist = string.IsNullOrEmpty(DisplayArtist) ? string.Empty : $"  {DisplayArtist}";
        return $"{PositionLabel}  {Title}{artist}  {DurationLabel}".Trim();
    }
}
