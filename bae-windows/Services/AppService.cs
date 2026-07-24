using System;
using Microsoft.UI.Dispatching;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// Root object for one open library — the Windows analog of BaeKit's
/// <c>AppService</c>. Built once around an already-open <see cref="SessionStore"/>,
/// it bundles the narrow domain services and the reactive stores that views and
/// stores consume instead of reaching for the session and <see cref="NativeBae"/>
/// directly. The session keeps its lifecycle role (open/subscribe/teardown); the
/// AppService is the thing built around it.
/// </summary>
internal sealed class AppService
{
    // Domain services: narrow, per-domain projections of the open handle that
    // views and stores read instead of the session + NativeBae directly.
    public LibraryService Library { get; }
    public MediaPathsService MediaPaths { get; }
    public PlaybackService Playback { get; }
    public QueueService Queue { get; }
    public DownloadsService Downloads { get; }
    public SyncService Sync { get; }

    // The system media transport controls, bound to the window's HWND at
    // construction. One instance for the window's lifetime; a library switch
    // deactivates it rather than recreating it.
    public MediaControlService MediaControlService { get; }

    // The reactive stores. One instance per open library: a switch builds a
    // fresh AppService with fresh stores (mirroring macOS's AppService).
    public ShellStore ShellStore { get; }
    public PlaybackStore PlaybackStore { get; }
    public CastStore CastStore { get; }
    public SyncStatusStore SyncStatusStore { get; }
    public SettingsStore SettingsStore { get; }
    public ImportStore ImportStore { get; }
    public StorageStore StorageStore { get; }
    public TransferProgressStore TransferProgressStore { get; }
    public ProjectionRegistry ProjectionRegistry { get; }
    public UiEventRouter UiEventRouter { get; }
    public LibraryBrowserStore LibraryBrowserStore { get; }

    // windowHandle is invoked once here (MediaControlService binds the HWND at
    // construction); the caller (MainWindow) has the live window handle.
    public AppService(SessionStore session, DispatcherQueue dispatcher, Func<IntPtr> windowHandle)
    {
        Library = LibraryService.FromSession(session);
        MediaPaths = MediaPathsService.FromSession(session);
        Playback = PlaybackService.FromSession(session);
        Queue = QueueService.FromSession(session);
        Downloads = DownloadsService.FromSession(session);
        Sync = SyncService.FromSession(session);

        ShellStore = new ShellStore();
        PlaybackStore = new PlaybackStore();
        CastStore = new CastStore(session);
        SyncStatusStore = new SyncStatusStore(session);
        // Built before the now-playing bar (in MainView): the bar reads the
        // remaining-time preference through the settings mirror, whose Current
        // stays null until a library seeds it.
        SettingsStore = new SettingsStore(session);
        MediaControlService = new MediaControlService(
            windowHandle(),
            dispatcher,
            Playback,
            imageId => MediaPaths.FetchCoverImageBytes(imageId));
        ImportStore = new ImportStore(session, ShellStore, MediaControlService);
        ProjectionRegistry = new ProjectionRegistry();
        // In-flight release transfers (pin/unpin/manage/unmanage), driven by
        // core's ReleaseTransferProgress/Ended events the router routes, read by
        // the storage dialog rows and the album-detail storage band. Built before
        // the router so it can route into it, and shared with the stores below.
        TransferProgressStore = new TransferProgressStore();
        UiEventRouter = new UiEventRouter(
            PlaybackStore,
            ShellStore,
            ProjectionRegistry,
            MediaControlService,
            ImportStore.HandlePreviewEvent,
            TransferProgressStore,
            CastStore);
        // The storage sheet's non-UI operations, shared by the storage dialog and
        // the album-detail storage band so both run transitions the same way.
        StorageStore = new StorageStore(session, TransferProgressStore);
        LibraryBrowserStore = new LibraryBrowserStore(Library, MediaPaths, dispatcher);
    }

    /// <summary>Report that a host UI screen opened, as a typed telemetry event
    /// through the process-lifetime sink. Infallible; the core owns every other
    /// event. The macOS AppService.reportScreen analog.</summary>
    public void ReportScreen(BridgeScreen screen) =>
        NativeBae.ReportScreen(BaeDiagnostics.Handle, screen);
}
