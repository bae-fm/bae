using System;
using Avalonia.Controls;

namespace Bae.Desktop;

/// <summary>A transport command the OS now-playing surface asked for. A surface
/// that offers no stop button never raises <see cref="Stop"/>.</summary>
internal enum MediaSessionCommand { Play, Pause, Next, Previous, Stop }

/// <summary>
/// The platform half of the OS now-playing surface: it publishes what
/// <see cref="MediaControlService"/> decided and raises what the OS asked for. It
/// makes no decisions — every push arrives already resolved, and every command
/// leaves as one of the transport commands above.
/// </summary>
internal interface IMediaSession : IDisposable
{
    /// <summary>Raised on whatever thread the platform surface uses;
    /// <see cref="MediaControlService"/> marshals onto the UI thread before
    /// acting.</summary>
    event Action<MediaSessionCommand>? CommandRequested;

    /// <summary>An absolute position, in milliseconds.</summary>
    event Action<double>? SeekRequestedMs;

    /// <summary>A volume in [0, 1] written by an OS client.</summary>
    event Action<double>? VolumeRequested;

    /// <summary>The window the surface attaches to, or null when no library
    /// window is up. Only the window-bound surfaces use it.</summary>
    void SetWindow(Window? window);

    /// <summary>Publish metadata and status, and mark the session live. The
    /// artwork slot follows the display's artwork action: Keep leaves the current
    /// image, Clear and Load both drop it immediately — under Load the bytes
    /// arrive later through <see cref="SetArtwork"/>.</summary>
    void Apply(MediaControlDisplay display);

    /// <summary>The image bytes for the load the last <see cref="Apply"/> asked
    /// for.</summary>
    void SetArtwork(byte[] bytes);

    /// <summary>Publish elapsed and total position. <paramref name="seeked"/>
    /// marks a position that jumped rather than advanced.</summary>
    void SetTimeline(MediaControlTimeline timeline, bool seeked);

    void SetCommandAvailability(bool hasNext, bool hasPrevious);

    /// <summary>The current output volume in [0, 1]; muted reports zero.</summary>
    void SetVolume(double volume);

    /// <summary>Put the surface back to "nothing playing".</summary>
    void Clear();
}
