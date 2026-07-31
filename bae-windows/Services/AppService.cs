using System;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml.Controls;
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
    public CastService Cast { get; }
    public SettingsService Settings { get; }
    public ImportService Import { get; }

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
        : this(
            session,
            dispatcher,
            windowHandle,
            LibraryService.FromSession(session),
            MediaPathsService.FromSession(session),
            PlaybackService.FromSession(session),
            QueueService.FromSession(session),
            DownloadsService.FromSession(session),
            SyncService.FromSession(session),
            CastService.FromSession(session),
            SettingsService.FromSession(session),
            ImportService.FromSession(session))
    {
    }

    // The domain services are injected so a scene can override them (see Stubbed);
    // the stores, transport controls, and router are the same for every
    // composition and are built here around the given session and services.
    private AppService(
        SessionStore session,
        DispatcherQueue dispatcher,
        Func<IntPtr> windowHandle,
        LibraryService library,
        MediaPathsService mediaPaths,
        PlaybackService playback,
        QueueService queue,
        DownloadsService downloads,
        SyncService sync,
        CastService cast,
        SettingsService settings,
        ImportService import)
    {
        Library = library;
        MediaPaths = mediaPaths;
        Playback = playback;
        Queue = queue;
        Downloads = downloads;
        Sync = sync;
        Cast = cast;
        Settings = settings;
        Import = import;

        ShellStore = new ShellStore();
        PlaybackStore = new PlaybackStore();
        CastStore = new CastStore(Cast);
        SyncStatusStore = new SyncStatusStore(Sync);
        // Built before the now-playing bar (in MainView): the bar reads the
        // remaining-time preference through the settings mirror, whose Current
        // stays null until a library seeds it.
        SettingsStore = new SettingsStore(Settings);
        MediaControlService = new MediaControlService(
            windowHandle(),
            dispatcher,
            Playback,
            image => MediaPaths.FetchLibraryImageBytes(image));
        ImportStore = new ImportStore(Import, ShowError, MediaControlService);
        ProjectionRegistry = new ProjectionRegistry();
        // In-flight release transfers (pin/unpin/manage/unmanage), driven by
        // core's ReleaseTransferProgress/Ended events the router routes, read by
        // the storage dialog rows and the album-detail storage band. Built before
        // the router so it can route into it, and shared with the stores below.
        TransferProgressStore = new TransferProgressStore();
        UiEventRouter = new UiEventRouter(
            PlaybackStore,
            ShowError,
            ProjectionRegistry,
            MediaControlService,
            ImportStore.HandlePreviewEvent,
            TransferProgressStore,
            CastStore);
        // The storage sheet's non-UI operations, shared by the storage dialog and
        // the album-detail storage band so both run transitions the same way.
        StorageStore = new StorageStore(Downloads, Sync, TransferProgressStore);
        LibraryBrowserStore = new LibraryBrowserStore(Library, MediaPaths, dispatcher);
    }

#if DEBUG
    /// <summary>A scene composition for the screenshot gallery: every domain
    /// service is a fail-loud stub except the caller-supplied <paramref name="library"/>,
    /// so a render that touches any unwired delegate crashes the capture loudly
    /// rather than drawing a lie. The stores are built around a handle-less
    /// <paramref name="session"/>; the shell renders its content off the stub
    /// library alone and never reaches a live handle.</summary>
    public static AppService Stubbed(
        SessionStore session,
        DispatcherQueue dispatcher,
        Func<IntPtr> windowHandle,
        LibraryService library) =>
        new(
            session,
            dispatcher,
            windowHandle,
            library,
            new MediaPathsService(),
            new PlaybackService(),
            new QueueService(),
            new DownloadsService(),
            new SyncService(),
            new CastService(),
            new SettingsService(),
            new ImportService());
#endif

    /// <summary>Route a caught error to the shell's error banner — the macOS
    /// AppService.showError analog, the single door feature code reaches for
    /// instead of writing the shell store's banner directly. Every caller surfaces
    /// an error, so the severity is fixed.</summary>
    public void ShowError(string title, string message) =>
        ShellStore.ShowBanner(InfoBarSeverity.Error, title, message);

    /// <summary>Report that a host UI screen opened, as a typed telemetry event
    /// through the process-lifetime sink. Infallible; the core owns every other
    /// event. The macOS AppService.reportScreen analog.</summary>
    public void ReportScreen(BridgeScreen screen) =>
        NativeBae.ReportScreen(BaeDiagnostics.Handle, screen);
}
