using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Root object for one open library — the desktop analog of BaeKit's
/// <c>AppService</c>. Built once around an already-open <see cref="SessionStore"/>,
/// it bundles the narrow domain services and the reactive stores that views and
/// stores consume instead of reaching for the session and <see cref="NativeBae"/>
/// directly. The session keeps its lifecycle role (open/subscribe/teardown); the
/// AppService is the thing built around it.
/// </summary>
internal sealed class AppService : IDisposable
{
    // The open session — the handle lifecycle the services and stores are built
    // around. Views read it only to guard an action on "is a library open?"
    // (CurrentHandleOrNull); everything library-domain goes through the services.
    public SessionStore Session { get; }

    // Domain services: narrow, per-domain projections of the open handle that
    // views and stores read instead of the session + NativeBae directly.
    public LibraryService Library { get; }
    public ImageStore Images { get; }
    public PlaybackService Playback { get; }
    public QueueService Queue { get; }
    public DownloadsService Downloads { get; }
    public SyncService Sync { get; }
    public CastService Cast { get; }
    public SettingsService Settings { get; }
    public ImportService Import { get; }
    public ReleaseEditorService ReleaseEditor { get; }
    public DiscogsService Discogs { get; }
    public AutomationService Automation { get; }
    public SubsonicService Subsonic { get; }

    // The OS now-playing / media-key surface. Process-scoped and handed in, not
    // built here: it owns a bus name (MPRIS) or a system session (SMTC) that
    // outlives any one open library, and a library switch deactivates it rather
    // than recreating it.
    public IMediaControl MediaControl { get; }

    // The library browser's data layer over the incremental-loading core: the
    // album / composer / artist paginated lists and their sort. One per open
    // library, like the other stores.
    public LibraryBrowserStore LibraryBrowserStore { get; }

    // The reactive stores. One instance per open library: a switch builds a fresh
    // AppService with fresh stores (mirroring macOS's AppService).
    public ShellStore ShellStore { get; }
    public PlaybackStore PlaybackStore { get; }
    public CastStore CastStore { get; }
    public SyncStatusStore SyncStatusStore { get; }
    public SettingsStore SettingsStore { get; }
    public ImportStore ImportStore { get; }
    public StorageStore StorageStore { get; }
    public UiEventRouter UiEventRouter { get; }
    private readonly ValueSubscriptions _valueSubscriptions;

    public AppService(SessionStore session, Dispatcher dispatcher, IMediaControl mediaControl)
        : this(
            session,
            dispatcher,
            mediaControl,
            LibraryService.FromSession(session),
            ImageStore.FromSession(session),
            PlaybackService.FromSession(session),
            QueueService.FromSession(session),
            DownloadsService.FromSession(session),
            SyncService.FromSession(session),
            CastService.FromSession(session),
            SettingsService.FromSession(session),
            ImportService.FromSession(session),
            ReleaseEditorService.FromSession(session),
            DiscogsService.FromSession(session),
            AutomationService.FromSession(session),
            SubsonicService.FromSession(session))
    {
    }

    // The domain services and the now-playing surface are injected so a scene can
    // override them (see Stubbed); the stores and the router are the same for
    // every composition and are built here around the given session and services.
    private AppService(
        SessionStore session,
        Dispatcher dispatcher,
        IMediaControl mediaControl,
        LibraryService library,
        ImageStore images,
        PlaybackService playback,
        QueueService queue,
        DownloadsService downloads,
        SyncService sync,
        CastService cast,
        SettingsService settings,
        ImportService import,
        ReleaseEditorService releaseEditor,
        DiscogsService discogs,
        AutomationService automation,
        SubsonicService subsonic)
    {
        Session = session;
        MediaControl = mediaControl;
        Library = library;
        Images = images;
        Playback = playback;
        Queue = queue;
        Downloads = downloads;
        Sync = sync;
        Cast = cast;
        Settings = settings;
        Import = import;
        ReleaseEditor = releaseEditor;
        Discogs = discogs;
        Automation = automation;
        Subsonic = subsonic;

        ShellStore = new ShellStore();
        PlaybackStore = new PlaybackStore();
        CastStore = new CastStore(Cast);
        SyncStatusStore = new SyncStatusStore(Sync);
        // Built before the now-playing bar (in the shell): the bar reads the
        // remaining-time preference through the settings mirror, whose Current
        // stays null until a library seeds it.
        SettingsStore = new SettingsStore(Settings);
        ImportStore = new ImportStore(Import, ShowError, MediaControl);
        UiEventRouter = new UiEventRouter(
            PlaybackStore,
            ShowError,
            MediaControl,
            ImportStore.HandlePreviewEvent,
            CastStore);
        // The storage sheet's non-UI operations, shared by the storage dialog and
        // the album-detail storage band so both run transitions the same way.
        StorageStore = new StorageStore(Downloads, Sync);
        // The library browser's paginated lists. A page subscription failure routes
        // to the shell banner; a session-swap mid-fetch (an OperationCanceled) is
        // the list being replaced and is dropped.
        LibraryBrowserStore = new LibraryBrowserStore(Library, Images, dispatcher, HandleBrowseError);
        _valueSubscriptions = new ValueSubscriptions(
            session, dispatcher, SettingsStore, SyncStatusStore, PlaybackStore,
            StorageStore, CastStore, ImportStore, ShowError);
    }

    private void HandleBrowseError(Exception exception)
    {
        switch (exception)
        {
            case OperationCanceledException:
                return;
            case PageLoadException page:
                ShowError(Loc.Chrome("error.title"), page.Line ?? Loc.Chrome("library.load_failed"));
                return;
            default:
                ShowError(Loc.Chrome("error.title"), Loc.Chrome("library.load_failed"));
                BaeDiagnostics.Logger.Warning("A library page load failed.", exception);
                return;
        }
    }

#if DEBUG
    /// <summary>A scene composition for the shot-capture gallery: every domain
    /// service is a fail-loud stub except those supplied by the caller,
    /// so a render that touches any unwired delegate crashes the capture loudly
    /// rather than drawing a lie. The stores are built around a handle-less
    /// <paramref name="session"/>; the shell renders its content off the stub
    /// library alone and never reaches a live handle. The now-playing surface is
    /// the no-op one: a headless capture must not stand up a real OS session.</summary>
    public static AppService Stubbed(
        SessionStore session,
        Dispatcher dispatcher,
        LibraryService library) =>
        CreateStubbed(
            session,
            dispatcher,
            library,
            new ImportService(),
            new PlaybackService(),
            new SettingsService());

    public static AppService Stubbed(
        SessionStore session,
        Dispatcher dispatcher,
        LibraryService library,
        SettingsService settings) =>
        CreateStubbed(
            session,
            dispatcher,
            library,
            new ImportService(),
            new PlaybackService(),
            settings);

    public static AppService Stubbed(
        SessionStore session,
        Dispatcher dispatcher,
        LibraryService library,
        ImportService import,
        PlaybackService playback) =>
        CreateStubbed(
            session,
            dispatcher,
            library,
            import,
            playback,
            new SettingsService());

    private static AppService CreateStubbed(
        SessionStore session,
        Dispatcher dispatcher,
        LibraryService library,
        ImportService import,
        PlaybackService playback,
        SettingsService settings) =>
        new(
            session,
            dispatcher,
            new NoopMediaControl(),
            library,
            new ImageStore(),
            playback,
            new QueueService(),
            new DownloadsService(),
            new SyncService(),
            new CastService(),
            settings,
            import,
            new ReleaseEditorService(),
            new DiscogsService(),
            new AutomationService(),
            new SubsonicService());
#endif

    /// <summary>Route a caught error to the shell's error banner — the macOS
    /// AppService.showError analog, the single door feature code reaches for
    /// instead of writing the shell store's banner directly. Every caller surfaces
    /// an error, so the severity is fixed.</summary>
    public void ShowError(string title, string message) =>
        ShellStore.ShowBanner(BannerSeverity.Error, title, message);

    /// <summary>Report that a host UI screen opened, as a typed telemetry event
    /// through the process-lifetime sink. Infallible; the core owns every other
    /// event. The macOS AppService.reportScreen analog.</summary>
    public void ReportScreen(BridgeScreen screen) =>
        NativeBae.ReportScreen(BaeDiagnostics.Handle, screen);

    public void Dispose() => _valueSubscriptions.Dispose();
}
