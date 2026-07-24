using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

public sealed class LibrarySearchResults
{
    internal LibrarySearchResults(BridgeSearchResults results)
    {
        Albums = results.Albums.Select(album => new Album(album)).ToList();
        Artists = results.Artists.Select(artist => new ArtistSummary(artist)).ToList();
        Tracks = results.Tracks.Select(track => new TrackSearchResult(track)).ToList();
        Composers = results.Composers.Select(composer => new ComposerSummary(composer)).ToList();
        Works = results.Works.Select(work => new WorkSummary(work)).ToList();
    }

    public List<Album> Albums { get; }
    public List<ArtistSummary> Artists { get; }
    public List<TrackSearchResult> Tracks { get; }
    public List<ComposerSummary> Composers { get; }
    public List<WorkSummary> Works { get; }
}

public sealed class TrackSearchResult : INotifyPropertyChanged
{
    private readonly BridgeTrackSearchResult _track;
    private readonly CoverImage.Binding _cover;

    internal TrackSearchResult(BridgeTrackSearchResult track)
    {
        _track = track;
        _cover = new CoverImage.Binding(track.Cover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string Id => _track.Id;
    public string Title => _track.Title;
    public string AlbumId => _track.AlbumId;
    public string AlbumTitle => _track.AlbumTitle;
    public string ArtistName => _track.ArtistName;
    public string DurationLabel => BridgeDisplay.Clock(_track.DurationClock);

    internal void AttachCover(MediaPathsService mediaPaths, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(mediaPaths, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;
}

public sealed class ComposerSummary : INotifyPropertyChanged
{
    private readonly BridgeComposerSummary _composer;
    private readonly CoverImage.Binding _cover;

    internal ComposerSummary(BridgeComposerSummary composer)
    {
        _composer = composer;
        _cover = new CoverImage.Binding(composer.Image);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string ArtistId => _composer.ArtistId;
    public string Name => _composer.Name;
    public string? SortName => _composer.SortName;
    public long WorkCount => _composer.WorkCount;
    public long LinkedReleaseCount => _composer.LinkedReleaseCount;
    public long UnlinkedCreditCount => _composer.UnlinkedCreditCount;

    internal void AttachCover(MediaPathsService mediaPaths, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(mediaPaths, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;
    public string WorkCountText => Loc.Chrome("work.count", "count", Loc.Number(WorkCount));
}

public sealed class WorkSummary : INotifyPropertyChanged
{
    private readonly BridgeWorkSummary _work;
    private readonly CoverImage.Binding _cover;

    internal WorkSummary(BridgeWorkSummary work)
    {
        _work = work;
        _cover = new CoverImage.Binding(work.RepresentativeCover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Cover)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string WorkId => _work.WorkId;
    public string Title => _work.Title;
    public string? Disambiguation => _work.Disambiguation;
    public string? WorkType => _work.WorkType;
    public string? ParentWorkId => _work.ParentWorkId;
    public string? ComposerNames => _work.ComposerNames;
    public long LinkedReleaseCount => _work.LinkedReleaseCount;
    public string? RepresentativeReleaseId => _work.RepresentativeReleaseId;

    internal void AttachCover(MediaPathsService mediaPaths, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(mediaPaths, dispatcherQueue);

    public ImageSource? Cover => _cover.Source;
}

public sealed class ComposerWorkGroup
{
    internal ComposerWorkGroup(BridgeComposerWorkGroup group)
    {
        Id = group.Id;
        Parent = group.Parent is null ? null : new WorkSummary(group.Parent);
        Works = group.Works.Select(work => new WorkSummary(work)).ToList();
    }

    public string Id { get; }
    public WorkSummary? Parent { get; }
    public List<WorkSummary> Works { get; }
}

public sealed class ComposerDetail
{
    internal ComposerDetail(BridgeComposerDetail detail)
    {
        Composer = new ComposerSummary(detail.Composer);
        WorkGroups = detail.WorkGroups.Select(group => new ComposerWorkGroup(group)).ToList();
        UnlinkedReleaseRoles = detail.UnlinkedReleaseRoles.Select(role => new ReleaseRoleSummary(role)).ToList();
        UnlinkedTrackRoles = detail.UnlinkedTrackRoles.Select(role => new TrackRoleSummary(role)).ToList();
        DefaultWorkId = detail.DefaultWorkId;
    }

    public ComposerSummary Composer { get; }
    public List<ComposerWorkGroup> WorkGroups { get; }
    public List<ReleaseRoleSummary> UnlinkedReleaseRoles { get; }
    public List<TrackRoleSummary> UnlinkedTrackRoles { get; }
    public string? DefaultWorkId { get; }
}

public sealed class ReleaseRoleSummary
{
    private readonly BridgeReleaseRoleSummary _role;

    internal ReleaseRoleSummary(BridgeReleaseRoleSummary role)
    {
        _role = role;
    }

    public string ReleaseId => _role.ReleaseId;
    public string AlbumId => _role.AlbumId;
    public string AlbumTitle => _role.AlbumTitle;
    public string? SourceCredit => _role.SourceCredit;
}

public sealed class TrackRoleSummary
{
    private readonly BridgeTrackRoleSummary _role;

    internal TrackRoleSummary(BridgeTrackRoleSummary role)
    {
        _role = role;
    }

    public string TrackId => _role.TrackId;
    public string TrackTitle => _role.TrackTitle;
    public string ReleaseId => _role.ReleaseId;
    public string AlbumId => _role.AlbumId;
    public string AlbumTitle => _role.AlbumTitle;
    public string ArtistId => _role.ArtistId;
    public string ArtistName => _role.ArtistName;
    public string? SourceCredit => _role.SourceCredit;
}

public sealed class WorkReleaseSummary : INotifyPropertyChanged
{
    private readonly BridgeWorkReleaseSummary _release;
    private readonly CoverImage.Binding _cover;

    internal WorkReleaseSummary(BridgeWorkReleaseSummary release)
    {
        _release = release;
        _cover = new CoverImage.Binding(release.Cover);
        _cover.SourceChanged += () => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(CoverImage)));
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    public string ReleaseId => _release.ReleaseId;
    public string AlbumId => _release.AlbumId;
    public string AlbumTitle => _release.AlbumTitle;
    public string DisplayName => _release.DisplayName;
    public string? Format => _release.Format;

    internal void AttachCover(MediaPathsService mediaPaths, DispatcherQueue dispatcherQueue) =>
        _cover.Attach(mediaPaths, dispatcherQueue);

    public ImageSource? CoverImage => _cover.Source;
    public string DisplaySubtitle =>
        string.IsNullOrWhiteSpace(Format)
            ? DisplayName
            : $"{DisplayName} · {Format}";
}

public sealed class WorkTrackSummary
{
    private readonly BridgeWorkTrackSummary _track;

    internal WorkTrackSummary(BridgeWorkTrackSummary track)
    {
        _track = track;
    }

    public string TrackId => _track.TrackId;
    public string TrackTitle => _track.TrackTitle;
    public string ReleaseId => _track.ReleaseId;
    public string AlbumId => _track.AlbumId;
    public string AlbumTitle => _track.AlbumTitle;
}

public sealed class WorkDetail
{
    internal WorkDetail(BridgeWorkDetail detail)
    {
        Work = new WorkSummary(detail.Work);
        ChildWorks = detail.ChildWorks.Select(work => new WorkSummary(work)).ToList();
        Releases = detail.Releases.Select(release => new WorkReleaseSummary(release)).ToList();
        Tracks = detail.Tracks.Select(track => new WorkTrackSummary(track)).ToList();
    }

    public WorkSummary Work { get; }
    public List<WorkSummary> ChildWorks { get; }
    public List<WorkReleaseSummary> Releases { get; }
    public List<WorkTrackSummary> Tracks { get; }
}
