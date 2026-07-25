namespace Bae.Desktop;

/// <summary>
/// The OS now-playing / media-key surface, consumed where the WinUI app consumed
/// <c>MediaControlService</c>. The decision logic stays in the pure
/// <see cref="MediaControlState"/>; an implementation only translates the pushes
/// into the platform transport (SMTC on Windows, MPRIS on Linux). Those backends
/// land with the parity port; until then <see cref="NoopMediaControl"/> stands in
/// so the stores that push to this surface compose without one.
/// </summary>
internal interface IMediaControl
{
    void UpdateNowPlayingPlaying(
        string trackTitle, string artistNames, string albumTitle, string? coverImageId, ulong durationMs);

    void UpdateNowPlayingPaused(
        string trackTitle, string artistNames, string albumTitle, string? coverImageId, ulong durationMs);

    void UpdateNowPlayingLoading(
        string trackTitle, string artistNames, string albumTitle, string? coverImageId, ulong durationMs);

    void UpdateNowPlayingStopped();

    void UpdatePosition(ulong positionMs, ulong durationMs);

    void UpdateCommandAvailability(bool hasNext, bool hasPrevious);

    void UpdateNowPlayingForPreview(string path, ulong durationMs, bool isPlaying);

    void UpdatePreviewIdle();

    void UpdatePreviewPosition(ulong positionMs);

    void Deactivate();
}

/// <summary>The stand-in transport: it drops every push. In place until the
/// Windows (SMTC) and Linux (MPRIS) backends land in the parity port, so the app
/// composes and runs with no OS now-playing surface rather than none at all.</summary>
internal sealed class NoopMediaControl : IMediaControl
{
    public void UpdateNowPlayingPlaying(
        string trackTitle, string artistNames, string albumTitle, string? coverImageId, ulong durationMs)
    {
    }

    public void UpdateNowPlayingPaused(
        string trackTitle, string artistNames, string albumTitle, string? coverImageId, ulong durationMs)
    {
    }

    public void UpdateNowPlayingLoading(
        string trackTitle, string artistNames, string albumTitle, string? coverImageId, ulong durationMs)
    {
    }

    public void UpdateNowPlayingStopped()
    {
    }

    public void UpdatePosition(ulong positionMs, ulong durationMs)
    {
    }

    public void UpdateCommandAvailability(bool hasNext, bool hasPrevious)
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
}
