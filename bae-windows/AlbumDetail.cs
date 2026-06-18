using System.Collections.Generic;

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

    /// <summary>The picker label.</summary>
    public override string ToString() => DisplayName;
}

/// <summary>One track row in an album's detail.</summary>
public sealed class Track
{
    public string TrackId { get; set; } = string.Empty;
    public string Title { get; set; } = string.Empty;
    public string Position { get; set; } = string.Empty;
    public string Duration { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;

    /// <summary>The list row; used as the default item text.</summary>
    public override string ToString() => $"{Position}  {Title}  {Duration}".Trim();
}
