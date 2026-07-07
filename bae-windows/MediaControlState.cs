using System;

namespace Bae.Windows;

/// <summary>How the current session's state renders on the system transport
/// controls. Mirrors WinRT's MediaPlaybackStatus, mirrored here so this layer
/// compiles without the Windows SDK projection. Closed has no counterpart: a
/// cleared display never goes through a <see cref="MediaControlDisplay"/> —
/// the shell clears the system session directly.</summary>
internal enum MediaControlPlaybackStatus { Changing, Paused, Playing }

/// <summary>What the artwork slot should do after a metadata update: keep the
/// current thumbnail (same image), clear it (no cover), or load a new image id.</summary>
internal abstract record MediaControlArtwork
{
    internal sealed record Keep : MediaControlArtwork;
    internal sealed record Clear : MediaControlArtwork;
    internal sealed record Load(string ImageId) : MediaControlArtwork;
}

/// <summary>One push to the system display: status, the metadata fields, and
/// the artwork action. Artist and album are empty for preview clips.</summary>
internal sealed record MediaControlDisplay(
    MediaControlPlaybackStatus Status,
    string Title,
    string Artist,
    string AlbumTitle,
    MediaControlArtwork Artwork);

/// <summary>A timeline push: elapsed and total, in milliseconds.</summary>
internal sealed record MediaControlTimeline(ulong PositionMs, ulong DurationMs);

/// <summary>Decides what the system media transport controls should show and
/// whether commands act on the library queue or a preview clip. Pure state
/// machine over the bridge's playback events; the WinRT calls live in
/// MediaControlService.</summary>
internal sealed class MediaControlState
{
    private bool _isShowingPreview;
    private bool _hasDisplay;          // something has been pushed since the last Clear
    private ulong? _currentDurationMs;
    private string? _artworkImageId;   // image id currently loaded/loading into the thumbnail

    /// <summary>Whether a preview clip is showing, so button/seek commands route
    /// to the preview instead of the library queue.</summary>
    internal bool IsShowingPreview => _isShowingPreview;

    /// <summary>The duration the current display tracks, or null when nothing is
    /// shown. The seek handler projects the optimistic timeline against it.</summary>
    internal ulong? CurrentDurationMs => _currentDurationMs;

    /// <summary>The display for a library track, or null while a preview is
    /// showing (a preview clip takes over the now-playing slot). The caller only
    /// invokes this for a playing or paused track, or a loading target whose
    /// metadata core has resolved; a bare loading event keeps the existing
    /// display untouched.</summary>
    internal MediaControlDisplay? UpdateForTrack(
        string trackTitle,
        string artistNames,
        string albumTitle,
        string? coverImageId,
        ulong durationMs,
        MediaControlPlaybackStatus status)
    {
        if (_isShowingPreview)
        {
            return null;
        }

        _currentDurationMs = durationMs;
        _hasDisplay = true;
        return new MediaControlDisplay(status, trackTitle, artistNames, albumTitle, ArtworkFor(coverImageId));
    }

    /// <summary>Resets every tracked field. The shell clears the system display
    /// unconditionally, so this returns nothing.</summary>
    internal void Clear()
    {
        _hasDisplay = false;
        _currentDurationMs = null;
        _artworkImageId = null;
    }

    /// <summary>A timeline push for the current library track, or null while a
    /// preview is showing or before anything has been displayed. Refreshes the
    /// duration used by <see cref="SeekRatio"/> from the event, which carries the
    /// pregap-adjusted value.</summary>
    internal MediaControlTimeline? UpdatePosition(ulong positionMs, ulong durationMs)
    {
        if (_isShowingPreview || !_hasDisplay)
        {
            return null;
        }

        _currentDurationMs = durationMs;
        return new MediaControlTimeline(positionMs, durationMs);
    }

    /// <summary>Enters preview mode and returns its display: the title is the
    /// clip's file name, artist and album are empty, and the artwork slot is
    /// cleared.</summary>
    internal MediaControlDisplay UpdateForPreview(string path, ulong durationMs, bool isPlaying)
    {
        _isShowingPreview = true;
        _hasDisplay = true;
        _currentDurationMs = durationMs;
        _artworkImageId = null;
        var status = isPlaying ? MediaControlPlaybackStatus.Playing : MediaControlPlaybackStatus.Paused;
        return new MediaControlDisplay(status, FileName(path), string.Empty, string.Empty, new MediaControlArtwork.Clear());
    }

    /// <summary>Leaves preview mode and resets every tracked field, like
    /// <see cref="Clear"/>.</summary>
    internal void ClearPreview()
    {
        _isShowingPreview = false;
        Clear();
    }

    /// <summary>A timeline push for the current preview clip, or null when nothing
    /// is previewing. The duration is the clip's stored duration.</summary>
    internal MediaControlTimeline? UpdatePreviewPosition(ulong positionMs)
    {
        if (!_isShowingPreview || _currentDurationMs is not { } durationMs)
        {
            return null;
        }

        return new MediaControlTimeline(positionMs, durationMs);
    }

    /// <summary>The seek ratio in [0, 1] for a requested position, or null when no
    /// duration is known — the command can't be honored without one.</summary>
    internal double? SeekRatio(double requestedPositionMs)
    {
        if (_currentDurationMs is not { } durationMs || durationMs == 0)
        {
            return null;
        }

        var ratio = requestedPositionMs / durationMs;
        return Math.Clamp(ratio, 0.0, 1.0);
    }

    /// <summary>Whether a completed artwork load for <paramref name="imageId"/> is
    /// still the current image — false once the track changed mid-load. Guards the
    /// async fetch from writing a stale thumbnail.</summary>
    internal bool ArtworkLoadIsCurrent(string imageId) => imageId == _artworkImageId;

    /// <summary>Records that the load for <paramref name="imageId"/> produced no
    /// thumbnail. Forgetting the id means the next update for the same cover
    /// issues a fresh <see cref="MediaControlArtwork.Load"/> instead of keeping a
    /// thumbnail that never arrived. A stale failure (the track already moved on)
    /// leaves the current load's tracking untouched.</summary>
    internal void ArtworkLoadFailed(string imageId)
    {
        if (imageId == _artworkImageId)
        {
            _artworkImageId = null;
        }
    }

    /// <summary>The artwork action for a track's cover: clear when it has none,
    /// keep when the id is unchanged (the thumbnail is already loaded), otherwise
    /// load the new id.</summary>
    private MediaControlArtwork ArtworkFor(string? coverImageId)
    {
        if (coverImageId is null)
        {
            _artworkImageId = null;
            return new MediaControlArtwork.Clear();
        }

        if (coverImageId == _artworkImageId)
        {
            return new MediaControlArtwork.Keep();
        }

        _artworkImageId = coverImageId;
        return new MediaControlArtwork.Load(coverImageId);
    }

    /// <summary>The last path segment, splitting on both separators so a Windows
    /// path resolves the same regardless of the host running this logic.</summary>
    private static string FileName(string path)
    {
        var separator = path.LastIndexOfAny(new[] { '/', '\\' });
        return separator < 0 ? path : path[(separator + 1)..];
    }
}
