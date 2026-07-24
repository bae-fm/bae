using System;

namespace Bae.Windows;

/// <summary>
/// Root object for one open library — the Windows analog of BaeKit's
/// <c>AppService</c>. Built once around an already-open <see cref="SessionStore"/>,
/// it bundles the narrow domain services (and, further on, the store bundle) that
/// views and stores consume instead of reaching for the session and
/// <see cref="NativeBae"/> directly. The session keeps its lifecycle role
/// (open/subscribe/teardown); the AppService is the thing built around it.
/// </summary>
internal sealed class AppService
{
    public LibraryService Library { get; }
    public MediaPathsService MediaPaths { get; }
    public PlaybackService Playback { get; }
    public QueueService Queue { get; }
    public DownloadsService Downloads { get; }
    public SyncService Sync { get; }

    public AppService(SessionStore session)
    {
        Library = LibraryService.FromSession(session);
        MediaPaths = MediaPathsService.FromSession(session);
        Playback = PlaybackService.FromSession(session);
        Queue = QueueService.FromSession(session);
        Downloads = DownloadsService.FromSession(session);
        Sync = SyncService.FromSession(session);
    }
}
