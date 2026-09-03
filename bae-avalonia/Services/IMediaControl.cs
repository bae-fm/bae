using System;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The OS now-playing / media-key surface. Core supplies the selected playback;
/// an implementation translates it into the platform transport (SMTC on Windows,
/// MPRIS on Linux). A composition
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

    void ApplyMediaControlValues(BridgeMediaControlValues values);

    void UpdateCommandAvailability(bool hasNext, bool hasPrevious);

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

    public void ApplyMediaControlValues(BridgeMediaControlValues values)
    {
    }

    public void UpdateCommandAvailability(bool hasNext, bool hasPrevious)
    {
    }

    public void Deactivate()
    {
    }

    public void Dispose()
    {
    }
}
