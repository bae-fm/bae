using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Playback transport — the C# mirror of BaeKit's <c>Playback</c> closure-struct,
/// plus the Windows-real reads/commands the transport needs (the volume read, the
/// repeat-cycle helper, the import preview transport). Commands carry the
/// session-swap currency the Windows session already exposes: a fire-and-forget
/// command returns whether a handle was current (so a caller that logs a dropped
/// command still can), a preference write returns its error line. Every delegate
/// defaults to a fail-loud stub; <see cref="FromSession"/> is the production
/// wiring.
/// </summary>
internal sealed class PlaybackService
{
    public Func<bool> Pause { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: Pause not wired");

    public Func<bool> Resume { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: Resume not wired");

    /// <summary>End playback and empty the now-playing slot. Distinct from
    /// <see cref="Pause"/>: the OS now-playing surfaces offer a stop the in-app
    /// transport does not.</summary>
    public Func<bool> Stop { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: Stop not wired");

    public Func<bool> NextTrack { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: NextTrack not wired");

    public Func<bool> PreviousTrack { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: PreviousTrack not wired");

    public Func<double, bool> SeekByRatio { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: SeekByRatio not wired");

    /// <summary>Seek the import audio preview by ratio.</summary>
    public Func<double, bool> PreviewSeekByRatio { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: PreviewSeekByRatio not wired");

    public Func<float, bool> SetVolume { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: SetVolume not wired");

    public Func<bool, bool> SetMuted { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: SetMuted not wired");

    /// <summary>Set the repeat mode to an absolute value; a cycling button passes
    /// the mode it wants, computed from <see cref="NextRepeatMode"/>.</summary>
    public Func<BridgeRepeatMode, bool> SetRepeatMode { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: SetRepeatMode not wired");

    /// <summary>The mode the repeat button steps to next — the cycle order is
    /// core's, so this is a pure bridge helper with no session.</summary>
    public Func<BridgeRepeatMode, BridgeRepeatMode> NextRepeatMode { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: NextRepeatMode not wired");

    public Func<string, long, bool, bool> PlayRelease { get; init; }
        = (_, _, _) => throw new InvalidOperationException("PlaybackService stub: PlayRelease not wired");

    /// <summary>Play several releases as one context, concatenated in order.</summary>
    public Func<IReadOnlyList<string>, bool> PlayReleases { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: PlayReleases not wired");

    /// <summary>Play the whole library in a fresh shuffle. An empty library is a
    /// no-op (logged in core).</summary>
    public Func<bool> PlayLibraryShuffled { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: PlayLibraryShuffled not wired");

    /// <summary>Whether the seek bar's leading label counts down remaining time.
    /// A synced preference, so the write returns its error line.</summary>
    public Func<bool, (bool Current, string? Error)> SetShowRemainingTime { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: SetShowRemainingTime not wired");

    /// <summary>Whether playback pauses between an album's sides. A synced
    /// preference.</summary>
    public Func<bool, (bool Current, string? Error)> SetPauseBetweenSides { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: SetPauseBetweenSides not wired");

    /// <summary>Start the import audio preview for a file path.</summary>
    public Func<string, bool> PreviewPlay { get; init; }
        = _ => throw new InvalidOperationException("PlaybackService stub: PreviewPlay not wired");

    public Func<bool> PreviewStop { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: PreviewStop not wired");

    public Func<bool> PreviewTogglePause { get; init; }
        = () => throw new InvalidOperationException("PlaybackService stub: PreviewTogglePause not wired");

    /// <summary>Wire every transport command through the open session's current handle.</summary>
    public static PlaybackService FromSession(SessionStore session) => new()
    {
        Pause = () => session.WithCurrentHandle(NativeBae.Pause),
        Resume = () => session.WithCurrentHandle(NativeBae.Resume),
        Stop = () => session.WithCurrentHandle(NativeBae.Stop),
        NextTrack = () => session.WithCurrentHandle(NativeBae.Next),
        PreviousTrack = () => session.WithCurrentHandle(NativeBae.Previous),
        SeekByRatio = ratio => session.WithCurrentHandle(handle => NativeBae.SeekByRatio(handle, ratio)),
        PreviewSeekByRatio = ratio => session.WithCurrentHandle(handle => NativeBae.PreviewSeekByRatio(handle, ratio)),
        SetVolume = volume => session.WithCurrentHandle(handle => NativeBae.SetVolume(handle, volume)),
        SetMuted = muted => session.WithCurrentHandle(handle => NativeBae.SetMuted(handle, muted)),
        SetRepeatMode = mode => session.WithCurrentHandle(handle => NativeBae.SetRepeatMode(handle, mode)),
        NextRepeatMode = mode => NativeBae.NextRepeatMode(mode),
        PlayRelease = (releaseId, startTrackIndex, shuffle) =>
            session.WithCurrentHandle(handle => NativeBae.PlayRelease(handle, releaseId, startTrackIndex, shuffle)),
        PlayReleases = releaseIds => session.WithCurrentHandle(handle => NativeBae.PlayReleases(handle, releaseIds)),
        PlayLibraryShuffled = () => session.WithCurrentHandle(NativeBae.PlayLibraryShuffled),
        SetShowRemainingTime = enabled =>
            session.WithCurrentHandle(handle => NativeBae.SetShowRemainingTime(handle, enabled)),
        SetPauseBetweenSides = enabled =>
            session.WithCurrentHandle(handle => NativeBae.SetPauseBetweenSides(handle, enabled)),
        PreviewPlay = path => session.WithCurrentHandle(handle => NativeBae.PreviewPlay(handle, path)),
        PreviewStop = () => session.WithCurrentHandle(NativeBae.PreviewStop),
        PreviewTogglePause = () => session.WithCurrentHandle(NativeBae.PreviewTogglePause),
    };
}
