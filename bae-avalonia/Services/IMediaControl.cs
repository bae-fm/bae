using System;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The OS now-playing / media-key surface. The decision logic stays in the pure
/// <see cref="MediaControlState"/>; an implementation only translates the pushes
/// into the platform transport (SMTC on Windows, MPRIS on Linux). A composition
/// with no OS surface — a headless capture, a scene stub — takes
/// <see cref="NoopMediaControl"/> instead.
/// </summary>
internal interface IMediaControl : IDisposable
{
    /// <summary>The window the surface attaches to, or null when no library
    /// window is up. The window coordinator drives this, because it is what swaps
    /// windows: a window driving its own attach on open and close would let the
    /// outgoing window's close detach the incoming window's surface.</summary>
    void SetWindow(Window? window);

    void UpdateNowPlayingPlaying(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs);

    void UpdateNowPlayingPaused(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs);

    void UpdateNowPlayingLoading(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs);

    void UpdateNowPlayingStopped();

    /// <summary>A position that advanced with playback.</summary>
    void UpdatePosition(ulong positionMs, ulong durationMs);

    /// <summary>A position that jumped, as opposed to advanced. The surfaces that
    /// announce seeks separately report only this one.</summary>
    void UpdateSeekedPosition(ulong positionMs, ulong durationMs);

    void UpdateCommandAvailability(bool hasNext, bool hasPrevious);

    /// <summary>The current output volume in [0, 1] and whether output is muted.
    /// The surfaces that expose a writable volume publish it; the rest ignore
    /// it.</summary>
    void UpdateVolume(float volume, bool isMuted);

    void UpdateNowPlayingForPreview(string path, ulong durationMs, bool isPlaying);

    void UpdatePreviewIdle();

    void UpdatePreviewPosition(ulong positionMs);

    void Deactivate();
}

/// <summary>The stand-in transport: it drops every push. Composed where standing
/// up a real OS session would be wrong — the headless shot capture and the scene
/// stubs — so those compose and render with no OS now-playing surface.</summary>
internal sealed class NoopMediaControl : IMediaControl
{
    public void SetWindow(Window? window)
    {
    }

    public void UpdateNowPlayingPlaying(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs)
    {
    }

    public void UpdateNowPlayingPaused(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs)
    {
    }

    public void UpdateNowPlayingLoading(
        string trackTitle, string artistNames, string albumTitle, BridgeImageRef? coverImage, ulong durationMs)
    {
    }

    public void UpdateNowPlayingStopped()
    {
    }

    public void UpdatePosition(ulong positionMs, ulong durationMs)
    {
    }

    public void UpdateSeekedPosition(ulong positionMs, ulong durationMs)
    {
    }

    public void UpdateCommandAvailability(bool hasNext, bool hasPrevious)
    {
    }

    public void UpdateVolume(float volume, bool isMuted)
    {
    }

    public void UpdateNowPlayingForPreview(string path, ulong durationMs, bool isPlaying)
    {
    }

    public void UpdatePreviewIdle()
    {
    }

    public void UpdatePreviewPosition(ulong positionMs)
    {
    }

    public void Deactivate()
    {
    }

    public void Dispose()
    {
    }
}
