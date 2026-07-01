using System;
using System.Collections.Generic;
using System.Text.Json.Serialization;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

public sealed class LibrarySearchResults
{
    public required List<Album> Albums { get; set; }
    public required List<TrackSearchResult> Tracks { get; set; }
    public required List<ComposerSummary> Composers { get; set; }
    public required List<WorkSummary> Works { get; set; }
}

public sealed class TrackSearchResult
{
    public required string Id { get; set; }
    public required string Title { get; set; }
    public long? DurationMs { get; set; }
    public required string AlbumId { get; set; }
    public required string AlbumTitle { get; set; }
    public required string ArtistName { get; set; }
    public string DurationLabel => Loc.Duration(DurationMs);
}

public sealed class ComposerSummary
{
    public required string ArtistId { get; set; }
    public required string Name { get; set; }
    public string? SortName { get; set; }
    public long WorkCount { get; set; }
    public long LinkedReleaseCount { get; set; }
    public long UnlinkedCreditCount { get; set; }
    public ImageRef? Image { get; set; }

    [JsonIgnore]
    public IntPtr Handle { get; set; }

    public ImageSource? Cover => CoverImage.LoadByImageRef(Handle, Image);
    public string WorkCountText => Loc.Chrome("work.count", "count", Loc.Number(WorkCount));
}

public sealed class WorkSummary
{
    public required string WorkId { get; set; }
    public required string Title { get; set; }
    public string? Disambiguation { get; set; }
    public string? WorkType { get; set; }
    public string? ParentWorkId { get; set; }
    public string? ComposerNames { get; set; }
    public long LinkedReleaseCount { get; set; }
    public string? RepresentativeReleaseId { get; set; }
    public ImageRef? RepresentativeCover { get; set; }

    [JsonIgnore]
    public IntPtr Handle { get; set; }

    public ImageSource? Cover => CoverImage.LoadByImageRef(Handle, RepresentativeCover);
}

public sealed class ComposerWorkGroup
{
    public required string Id { get; set; }
    public WorkSummary? Parent { get; set; }
    public required List<WorkSummary> Works { get; set; }
}

public sealed class ComposerDetail
{
    public required ComposerSummary Composer { get; set; }
    public required List<ComposerWorkGroup> WorkGroups { get; set; }
    public required List<ReleaseRoleSummary> UnlinkedReleaseRoles { get; set; }
    public required List<TrackRoleSummary> UnlinkedTrackRoles { get; set; }
    public string? DefaultWorkId { get; set; }
}

public sealed class ReleaseRoleSummary
{
    public required string ReleaseId { get; set; }
    public required string AlbumId { get; set; }
    public required string AlbumTitle { get; set; }
    public string? SourceCredit { get; set; }
}

public sealed class TrackRoleSummary
{
    public required string TrackId { get; set; }
    public required string TrackTitle { get; set; }
    public required string ReleaseId { get; set; }
    public required string AlbumId { get; set; }
    public required string AlbumTitle { get; set; }
    public required string ArtistId { get; set; }
    public required string ArtistName { get; set; }
    public string? SourceCredit { get; set; }
}

public sealed class WorkReleaseSummary
{
    public required string ReleaseId { get; set; }
    public required string AlbumId { get; set; }
    public required string AlbumTitle { get; set; }
    public required string DisplayName { get; set; }
    public string? Format { get; set; }
    public ImageRef? Cover { get; set; }

    [JsonIgnore]
    public IntPtr Handle { get; set; }

    public ImageSource? CoverImage => Bae.Windows.CoverImage.LoadByImageRef(Handle, Cover);
    public string DisplaySubtitle =>
        string.IsNullOrWhiteSpace(Format)
            ? DisplayName
            : $"{DisplayName} · {Format}";
}

public sealed class WorkTrackSummary
{
    public required string TrackId { get; set; }
    public required string TrackTitle { get; set; }
    public required string ReleaseId { get; set; }
    public required string AlbumId { get; set; }
    public required string AlbumTitle { get; set; }
}

public sealed class WorkDetail
{
    public required WorkSummary Work { get; set; }
    public required List<WorkSummary> ChildWorks { get; set; }
    public required List<WorkReleaseSummary> Releases { get; set; }
    public required List<WorkTrackSummary> Tracks { get; set; }
}
