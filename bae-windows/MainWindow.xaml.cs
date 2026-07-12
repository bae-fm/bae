using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

/// <summary>
/// The library grid. On launch it discovers the libraries on disk, opens the
/// active one (or the first), and loads the first page of albums (newest first)
/// into the grid. With no library present it offers to create one; with none
/// discoverable (as in CI) the discovery list is empty and the same path runs.
///
/// The handle is held for the window's lifetime (playback, events, and later
/// screens reuse it) and released when the window closes.
/// </summary>
public sealed partial class MainWindow : Window
{
    // The library session (handle + event subscription) and the stores it drives.
    private readonly SessionStore _session;
    private readonly LibraryBrowserStore _browser;
    // The album grid's multi-selection, a browsing-session concern owned here
    // like the browser store itself. Modifier clicks/Esc/Ctrl+A mutate it; the
    // window syncs Album.IsSelected over the loaded collection after every
    // mutation so the card tint (bound OneWay) stays current.
    private readonly AlbumGridSelectionModel _albumSelection = new();
    private readonly LibrarySortControls _sortControls;
    private readonly BrowserPanes _browserPanes;
    private readonly WelcomeView _welcomeView;
    private readonly LibrariesDialog _librariesDialog;
    private readonly JoinLibraryDialog _joinDialog;
    private readonly ApproveDeviceDialog _approveDialog;
    private readonly UnlockDialog _unlockDialog;
    private readonly RestoreFromCloudDialog _restoreCloudDialog;
    private readonly ShellStore _shell;
    private readonly SyncStatusStore _sync;
    private readonly PlaybackStore _playback;
    private readonly ProjectionRegistry _projections;
    private readonly TransferProgressStore _transferProgress;
    private readonly UiEventRouter _router;
    private readonly NowPlayingBarController _nowPlayingBar;
    private readonly ImportStore _import;
    private readonly ImportDialog _importDialog;
    private readonly ImportPickerDialog _importPicker;
    private readonly ImportConfirmDialog _importConfirm;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly AlbumDetailDialog _albumDetail;
    private readonly QueuePane _queuePane;
    private readonly LightboxOverlay _lightbox;
    private readonly StorageStore _storage;
    private readonly StorageDialog _storageDialog;
    private readonly SettingsStore _settings;
    private readonly MembersPane _membersPane;
    private readonly SettingsDialog _settingsDialog;

    // Drives the system media transport controls (hardware media keys + the
    // Windows media flyout) from playback events. One instance for the window's
    // lifetime; library switches deactivate it rather than recreating it.
    private readonly MediaControlService _mediaControls;

    // Drives the settings "updates" section: a launch-time background check and
    // the manual check/download/apply from settings. Inert on a dev run or a
    // loose-zip copy (IsAvailable is false).
    private readonly UpdateService _updateService = new();

    // The window's last-seen normal (restored-state) bounds and whether it is
    // maximized, tracked from AppWindow.Changed and written once in OnClosed.
    // Normal bounds are recorded even while maximized, so restoring down after a
    // relaunch lands on the user's chosen size rather than a display-sized rect.
    private PixelRect? _lastNormalBounds;
    private bool _maximized;

    // Set once OnClosed's first pass has run its teardown. `async void` means
    // the window would otherwise die mid-await on the first `await` inside
    // the handler; cancelling that first close (`args.Handled = true`), then
    // awaiting the teardown, then closing again lets it actually finish
    // before the process exits. See OnClosed.
    private bool _closeTeardownDone;

    // The launch-time activation intent (a folder verb / bae://import command
    // line, parsed by App.OnLaunched) latches here until the initial
    // library-open attempt settles — a cold-start activation can arrive before
    // an async unlock resolves, when CurrentHandleOrNull() would be null
    // spuriously. A redirected activation (the app already warm) dispatches
    // straight to HandleActivationIntent instead of going through this latch.
    private bool _initialLibraryOpenSettled;
    private ActivationIntent? _pendingLaunchIntent;

    [DllImport("user32.dll")]
    private static extern bool SetForegroundWindow(IntPtr hWnd);

    // x:Bind Albums/Composers/Artists in the shell resolve here; the collections
    // live in the browser store (constructed before InitializeComponent so
    // these are non-null when the bindings first evaluate).
    public ObservableCollection<Album> Albums => _browser.Albums;
    public ObservableCollection<ComposerSummary> Composers => _browser.Composers;
    public ObservableCollection<ArtistSummary> Artists => _browser.Artists;

    public MainWindow()
    {
        // The browser store owns the Albums/Composers/Artists collections the
        // shell binds to with x:Bind, so it (and the session it reads) must
        // exist before InitializeComponent evaluates those bindings.
        _session = new SessionStore(DispatcherQueue);
        _browser = new LibraryBrowserStore(_session, DispatcherQueue);

        InitializeComponent();
        Closed += OnClosed;

        // Restore the window to its last-used position, size, and maximized state
        // before the app activates it, so it appears already in place with no
        // flicker. The saved bounds are clamped to the current displays; anything
        // unusable leaves the system's default placement. Track later moves and
        // resizes so OnClosed can persist the final normal bounds.
        RestoreWindowBounds();
        AppWindow.Changed += OnAppWindowChanged;

        // The window's HWND exists at construction, so bind the transport controls
        // to it now — before LoadLibrary starts the event stream that drives them.
        _mediaControls = new MediaControlService(
            WinRT.Interop.WindowNative.GetWindowHandle(this),
            DispatcherQueue,
            WithCurrentHandle,
            imageId =>
            {
                byte[]? bytes = null;
                WithCurrentHandle(handle => bytes = NativeBae.CoverImageBytes(handle, imageId));
                return bytes;
            });

        // Bind layout direction to the UI locale: ar/he (and any other RTL
        // culture) lay out right-to-left. The whole tree inherits from the root
        // grid, so this is the single place the app decides direction. macOS gets
        // this from the system; on Windows the app sets it from the culture.
        RootGrid.FlowDirection = CultureInfo.CurrentUICulture.TextInfo.IsRightToLeft
            ? FlowDirection.RightToLeft
            : FlowDirection.LeftToRight;

        BrowserModeBox.Items.Add(Loc.Chrome("library.mode.albums"));
        BrowserModeBox.Items.Add(Loc.Chrome("library.mode.artists"));
        BrowserModeBox.Items.Add(Loc.Chrome("library.mode.composers"));
        _sortControls = new LibrarySortControls(SortControls, _browser.Sort, ReloadBrowserForSortChange);
        _sortControls.Render();
        BrowserModeBox.SelectedIndex = 0;

        _shell = new ShellStore();
        _shell.Changed += RenderBanner;
        _playback = new PlaybackStore();
        _sync = new SyncStatusStore(_session);
        _sync.Changed += RenderSyncStatus;
        _nowPlayingBar = new NowPlayingBarController(
            _session,
            _playback,
            () => Content.XamlRoot,
            NowPlayingBar,
            NpCover,
            NpTitle,
            NpArtist,
            NpElapsed,
            NpDuration,
            NpProgress,
            NpVolume,
            NpPlayPause,
            NpMute,
            NpRepeat,
            NpPrev,
            NpNext,
            NpLoading,
            QueueAddBadge,
            QueueAddBadgeScale,
            QueueAddBadgeText);
        // The bar's art and title open the playing album; the tooltip is the
        // affordance that they're clickable.
        ToolTipService.SetToolTip(NpCoverFrame, Loc.Chrome("nowplaying.go_to_album"));
        ToolTipService.SetToolTip(NpTitle, Loc.Chrome("nowplaying.go_to_album"));
        _import = new ImportStore(_session, _shell, _mediaControls);
        _projections = new ProjectionRegistry();
        // In-flight release transfers (pin/unpin/manage/unmanage), driven by core's
        // ReleaseTransferProgress/Ended events the router routes, read by the
        // storage dialog rows and the album-detail storage band. Constructed before
        // the router so it can route into it, and shared with the stores below.
        _transferProgress = new TransferProgressStore();
        _router = new UiEventRouter(
            _playback, _shell, _projections, _mediaControls, _import.HandlePreviewEvent, _transferProgress);
        _session.UiEvent += _router.Route;

        // The artwork lightbox: opened from the album gallery and the import
        // confirmation's local artwork tiles.
        _lightbox = new LightboxOverlay(() => Content.XamlRoot);

        // The storage sheet's non-UI operations, shared by the storage dialog and
        // the album-detail storage band so both run transitions the same way.
        _storage = new StorageStore(_session, _transferProgress);

        // Album detail and the per-release action dialogs it opens, plus the queue
        // dialog. Album detail is the shared entry point from the grid, the panes,
        // the now-playing jump, and the import "view in library" banner.
        _releaseActions = new ReleaseActionDialogs(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
            _projections,
            _lightbox);
        _albumDetail = new AlbumDetailDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
            _releaseActions,
            _storage,
            _transferProgress,
            _projections);
        _queuePane = new QueuePane(
            _session,
            _playback,
            QueuePaneHost,
            message => _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("error.playback_title"), message));

        // The storage sheet. The dialog registers its live-refresh handlers on the
        // projection registry while open.
        _storageDialog = new StorageDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            _storage,
            _projections,
            _transferProgress);

        // The composer/search panes and the library-lifecycle dialogs. The window
        // stays the navigation shell (open/close/switch); these render and drive
        // their own screens, calling back for the operations it owns.
        _browserPanes = new BrowserPanes(
            _session,
            DispatcherQueue,
            SearchResultsList,
            ComposerDetailPane,
            ArtistDetailPane,
            text => StatusText.Text = text,
            ShowComposerBrowser,
            ShowArtistBrowser,
            _albumDetail.Show);
        _unlockDialog = new UnlockDialog(() => Content.XamlRoot, text => StatusText.Text = text, OpenLibrary);
        _joinDialog = new JoinLibraryDialog(
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            () => _welcomeView.Dismiss(),
            OpenLibrary);
        _restoreCloudDialog = new RestoreFromCloudDialog(
            () => Content.XamlRoot,
            () => _welcomeView.Dismiss(),
            OpenLibrary);
        _approveDialog = new ApproveDeviceDialog(
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            _session);
        _librariesDialog = new LibrariesDialog(
            () => Content.XamlRoot,
            _session,
            LoadLibraries,
            CreateLibraryOrReport,
            SwitchLibrary);
        _welcomeView = new WelcomeView(
            EmptyState,
            text => StatusText.Text = text,
            LoadLibraries,
            CreateLibraryOrReport,
            OpenLibrary,
            () => _joinDialog.Show(),
            () => _restoreCloudDialog.Show());

        // The import flow: the confirm step (which can open an album), the picker
        // that leads to it, and the folder-scan dialog that opens the picker.
        _importConfirm = new ImportConfirmDialog(
            _session, () => Content.XamlRoot, albumId => _albumDetail.Show(albumId), _lightbox);
        _importPicker = new ImportPickerDialog(_session, () => Content.XamlRoot, _import, _importConfirm);
        _importDialog = new ImportDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            _import,
            _importPicker);

        // The settings dialog and its store/panes. It registers its config re-read
        // on the projection registry while open, opens the approve flow for
        // add-device, and shares the one UpdateService with the launch-time check.
        _settings = new SettingsStore(_session);
        _membersPane = new MembersPane(_session);
        _settingsDialog = new SettingsDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            DispatcherQueue,
            _settings,
            _membersPane,
            _approveDialog,
            _updateService,
            _projections,
            text => StatusText.Text = text,
            OpenLibrary,
            CloseLibrary);

        RegisterProjections();

        LoadLibrary();

        // Check for an update in the background at launch, like macOS's Sparkle
        // check-on-appear. Fire-and-forget: the service catches and logs every
        // failure and this is async I/O, so it never blocks startup.
        _ = _updateService.CheckInBackgroundAsync();
    }

    // Wire core's invalidations to the reloads they drive. Static consumers (the
    // album grid, sync status, import candidates) register for the window's
    // lifetime; the storage and settings dialogs supply their live-refresh
    // callbacks while open.
    private void RegisterProjections()
    {
        _projections.Register(typeof(BridgeInvalidation.AlbumList), ReloadBrowserFromInvalidation);
        _projections.Register(typeof(BridgeInvalidation.ComposerList), ReloadBrowserFromInvalidation);
        _projections.Register(typeof(BridgeInvalidation.ArtistList), ReloadBrowserFromInvalidation);
        _projections.Register(typeof(BridgeInvalidation.SyncStatus), _sync.Refresh);
        _projections.Register(typeof(BridgeInvalidation.ImportCandidateList), _import.RefreshCandidates);
        _projections.Register(typeof(BridgeInvalidation.ImportCandidate), _import.RefreshCandidates);
        _projections.Register(typeof(BridgeInvalidation.WatchedFolders), _import.RefreshCandidates);
    }

    private void ReloadBrowserFromInvalidation()
    {
        if (string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private void OnBrowserModeChanged(object sender, SelectionChangedEventArgs e)
    {
        _browser.Sort.SetMode(BrowserModeBox.SelectedIndex switch
        {
            1 => BrowserMode.Artists,
            2 => BrowserMode.Composers,
            _ => BrowserMode.Albums,
        });
        _sortControls.Render();
        ReloadBrowserForSortChange();
    }

    // Reload the active grid after a sort or mode change, but only when a library is
    // open and no search is active — search results keep their relevance order.
    private void ReloadBrowserForSortChange()
    {
        if (CurrentHandleOrNull() != null && string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private void ShowAlbumBrowser()
    {
        AlbumGrid.Visibility = Visibility.Visible;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        ArtistBrowser.Visibility = Visibility.Collapsed;
        SearchResultsList.Visibility = Visibility.Collapsed;
    }

    private void ShowComposerBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Visible;
        ArtistBrowser.Visibility = Visibility.Collapsed;
        SearchResultsList.Visibility = Visibility.Collapsed;
    }

    private void ShowArtistBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        ArtistBrowser.Visibility = Visibility.Visible;
        SearchResultsList.Visibility = Visibility.Collapsed;
    }

    private void ShowSearchBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        ArtistBrowser.Visibility = Visibility.Collapsed;
        SearchResultsList.Visibility = Visibility.Visible;
    }

    // Load the active mode's grid through the store, then render the status line
    // from what it returns. The composer pane flips into view and clears its detail
    // column up front (matching the original order) before the load can bail; the
    // album pane flips only once a load lands, so a handle-gone bail leaves it as-is.
    private void LoadCurrentBrowserMode()
    {
        if (_browser.Sort.Mode == BrowserMode.Composers)
        {
            ShowComposerBrowser();
            _browserPanes.ClearComposerDetail();
            RenderGridStatus(_browser.LoadComposers());
            return;
        }

        if (_browser.Sort.Mode == BrowserMode.Artists)
        {
            ShowArtistBrowser();
            _browserPanes.ClearArtistDetail();
            RenderGridStatus(_browser.LoadArtists());
            return;
        }

        var load = _browser.LoadAlbums();
        if (load.Result == BrowserLoadResult.HandleGone)
        {
            return;
        }
        // A reload replaces Albums wholesale (sort/mode change, invalidation,
        // library open), so the prior selection's ids no longer resolve to
        // anything on screen — clear it rather than prune, which also
        // subsumes macOS's per-id deleted-album pruning.
        _albumSelection.Clear();
        SyncAlbumSelectionTint();
        ShowAlbumBrowser();
        RenderGridStatus(load);
    }

    // Set the status line from a completed grid load; a handle-gone load leaves it
    // untouched. Visibility is the caller's concern.
    private void RenderGridStatus(BrowserGridLoad load)
    {
        // Albums are the proxy the UI owns for "anything to shuffle": core's
        // shuffle no-ops on zero tracks, and the album count is what this
        // window already loads. Only an album-mode load speaks to it (a
        // composer load says nothing about albums), and a handle-gone load
        // leaves the view untouched, this included. A failed load disables —
        // don't offer playback of unknown contents.
        if (load.Result != BrowserLoadResult.HandleGone && load.Mode == BrowserMode.Albums)
        {
            ShuffleLibraryItem.IsEnabled =
                load.Result == BrowserLoadResult.Loaded && !load.IsEmpty;
        }

        switch (load.Result)
        {
            case BrowserLoadResult.HandleGone:
                return;
            case BrowserLoadResult.Failed:
                StatusText.Text = load.Error ?? Loc.Chrome("library.load_failed");
                return;
            default:
                StatusText.Text = load.IsEmpty
                    ? Loc.Chrome(load.Mode switch
                    {
                        BrowserMode.Composers => "library.no_composers",
                        BrowserMode.Artists => "library.no_artists",
                        _ => "library.empty",
                    })
                    : string.Empty;
                return;
        }
    }

    // Create a new library, reporting failure through the caller's surface (the
    // welcome status line, or the library manager's). Returns the new id, or null
    // on failure. Callers diverge only in how they open it.
    private string? CreateLibraryOrReport(Action<string> reportError)
    {
        try
        {
            return NativeBae.CreateLibrary();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to create library.", exception);
            reportError(exception.Message);
            return null;
        }
    }

    // The libraries discovered on this device. Empty when discovery fails or none
    // exist; callers pick the active one, or list them.
    private List<BridgeLibrary> LoadLibraries()
    {
        try
        {
            return NativeBae.Libraries();
        }
        catch (BridgeException exception)
        {
            BaeDiagnostics.Logger.Error("Failed to discover libraries.", exception);
            StatusText.Text = exception.Message;
            return new List<BridgeLibrary>();
        }
    }

    private void LoadLibrary()
    {
        var libraries = LoadLibraries();
        var library = libraries.FirstOrDefault(candidate => candidate.IsActive)
            ?? libraries.FirstOrDefault();
        if (library is null)
        {
            _welcomeView.Show();
            // Settled: no library exists to hand a launch-time intent to, so
            // it applies now as the no-op HandleActivationIntent logs.
            SettleInitialLibraryOpen();
            return;
        }

        OpenLibrary(library.Id);
    }

    private void OpenLibrary(string libraryId)
    {
        switch (_session.OpenHandle(libraryId))
        {
            case OpenHandleResult.Failed:
                StatusText.Text = Loc.Chrome("library.open_failed");
                SettleInitialLibraryOpen();
                return;
            case OpenHandleResult.NeedsUnlock:
                // Encrypted library whose key isn't on this device: the session
                // freed the handle it made; prompt for the key rather than show a
                // half-open library. Unlocking re-opens with sync online, which
                // settles the latch below through this same method; until then
                // a pending launch intent stays latched.
                _ = _unlockDialog.Show(libraryId);
                return;
        }

        // Committed to showing this library: drop the welcome chooser if it's up.
        // Done here (not at the call sites) so a failed open or an unlock detour
        // above leaves the welcome in place rather than stranding the user.
        _welcomeView.Dismiss();

        LoadCurrentBrowserMode();
        _nowPlayingBar.SeedVolume();
        _sync.Refresh();
        _session.Subscribe();
        SettleInitialLibraryOpen();
    }

    // Set by App.OnLaunched once the window exists. Applies immediately when
    // the initial library-open attempt has already settled (the common case:
    // LoadLibrary runs synchronously in the constructor, before this is
    // called); otherwise latches until SettleInitialLibraryOpen runs.
    internal void SetPendingLaunchIntent(ActivationIntent intent)
    {
        if (_initialLibraryOpenSettled)
        {
            _ = HandleActivationIntent(intent);
        }
        else
        {
            _pendingLaunchIntent = intent;
        }
    }

    // Idempotent: called at every terminal point of the launch-time
    // LoadLibrary/OpenLibrary flow (no library found, a failed open, or a
    // successful open), including the one that runs after an async unlock
    // resolves. Only the first call after a pending intent was set has an
    // effect; later calls (a manual library switch, for instance) find nothing
    // latched.
    private void SettleInitialLibraryOpen()
    {
        _initialLibraryOpenSettled = true;
        if (_pendingLaunchIntent is { } intent)
        {
            _pendingLaunchIntent = null;
            _ = HandleActivationIntent(intent);
        }
    }

    // Turn a parsed activation intent (the folder verb, a dropped-on-the-exe
    // folder, or bae://import) into the same UI action OnWindowDrop runs for a
    // dropped folder.
    internal async System.Threading.Tasks.Task HandleActivationIntent(ActivationIntent intent)
    {
        switch (intent)
        {
            case ActivationIntent.ImportFolder importFolder:
                if (CurrentHandleOrNull() == null)
                {
                    // No open library (welcome screen or an unresolved unlock):
                    // a no-op, matching the macOS handler's folder add.
                    BaeDiagnostics.Logger.Info(
                        "Ignored a folder-import activation: no library is open.");
                    return;
                }
                await ImportFolder(importFolder.Path);
                return;
            default:
                throw new ArgumentOutOfRangeException(nameof(intent), intent, "Unknown activation intent");
        }
    }

    // Bring the window to the front for a redirected activation (a second
    // launch while bae is already running): restore it first if minimized.
    // The OS may downgrade this to a taskbar flash when foreground rights are
    // denied to a background process — accepted, not worked around.
    internal void BringToForeground()
    {
        if (AppWindow.Presenter is OverlappedPresenter { State: OverlappedPresenterState.Minimized } presenter)
        {
            presenter.Restore();
        }

        SetForegroundWindow(WinRT.Interop.WindowNative.GetWindowHandle(this));
    }

    // Render the toolbar sync indicator and the sync banner from the sync store's
    // current state (subscribed to its Changed).
    private void RenderSyncStatus()
    {
        if (_sync.ErrorText is null)
        {
            SyncBanner.IsOpen = false;
        }
        else
        {
            var reconnect = new Button { Content = Loc.Chrome("sync.reconnect") };
            reconnect.Click += (_, _) => WithCurrentHandle(NativeBae.TriggerSync);
            SyncBanner.Severity = InfoBarSeverity.Error;
            SyncBanner.Title = Loc.Chrome("sync.error_title");
            SyncBanner.Message = _sync.ErrorText;
            SyncBanner.ActionButton = reconnect;
            SyncBanner.IsOpen = true;
        }

        var indicator = SyncIndicatorModel.Resolve(_sync.ErrorText is not null, _sync.Syncing, _sync.LastSyncTime);
        switch (indicator.Kind)
        {
            case SyncIndicatorKind.Error:
                SyncIndicator.Text = Loc.Chrome("sync.error_title");
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                break;
            case SyncIndicatorKind.Syncing:
                SyncIndicator.Text = Loc.Chrome("sync.syncing");
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                break;
            case SyncIndicatorKind.Synced:
                SyncIndicator.Text = Loc.Chrome("sync.synced", "time", indicator.LastSyncTime);
                SyncIndicator.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                break;
            case SyncIndicatorKind.Blank:
                SyncIndicator.Text = string.Empty;
                break;
        }
    }

    // Render the shell error banner from the shell store (subscribed to Changed).
    private void RenderBanner()
    {
        Banner.Severity = _shell.BannerSeverity;
        Banner.Title = _shell.BannerTitle;
        Banner.Message = _shell.BannerMessage;
        Banner.ActionButton = null;
        Banner.IsOpen = _shell.BannerIsOpen;
    }

    // Switch the active library: persist the current one's playback state, tear
    // down its handle and view state, then open the target. generated bridge init records the
    // target as the active library once it opens unlocked; a locked target lands
    // on the unlock prompt. Used for switching to an existing library and for a
    // freshly created one.
    private async System.Threading.Tasks.Task SwitchLibrary(string libraryId)
    {
        await TearDownLibrary();
        OpenLibrary(libraryId);
    }

    // Tear down the open library: shut down and free its handle, and reset every
    // piece of per-library view state so nothing from it bleeds into the next
    // library or the welcome chooser. Leaves the window with no library open.
    private async System.Threading.Tasks.Task TearDownLibrary()
    {
        await _session.ShutdownAndFreeCurrentHandle();

        _playback.Reset();
        _nowPlayingBar.Reset();
        _queuePane.Hide();
        _import.Reset();
        _browser.Reset();
        _albumSelection.Clear();
        _transferProgress.Reset();
        SearchBox.Text = string.Empty;
        StatusText.Text = string.Empty;
        ShuffleLibraryItem.IsEnabled = false;
        _mediaControls.Deactivate();
        // The banners report the old library's sync / playback errors; clear them
        // so they don't describe state the next library (or none) doesn't have.
        _shell.ClearBanner();
        _sync.Reset();
    }

    // Close the open library and return to the welcome chooser, which now lists
    // the libraries on disk so the user can reopen one or create another.
    private async System.Threading.Tasks.Task CloseLibrary()
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        await TearDownLibrary();
        _welcomeView.Show();
    }

    private System.Threading.Tasks.Task ShutdownAndFreeCurrentHandle() =>
        _session.ShutdownAndFreeCurrentHandle();

    private bool WithCurrentHandle(Action<AppHandle> action) =>
        _session.WithCurrentHandle(action);

    private LibraryHandle? CurrentHandleOrNull() =>
        _session.CurrentHandleOrNull();

    private async void OnCloseLibraryClick(object sender, RoutedEventArgs e)
    {
        await CloseLibrary();
    }

    private void OnShuffleLibraryClick(object sender, RoutedEventArgs e)
    {
        WithCurrentHandle(NativeBae.PlayLibraryShuffled);
    }

    private async void OnLibrariesClick(object sender, RoutedEventArgs e)
    {
        await _librariesDialog.Show();
    }

    private async void OnImportClick(object sender, RoutedEventArgs e)
    {
        await _importDialog.Show();
    }

    // Accept a dragged folder anywhere over the window (matching macOS, which
    // imports a folder dropped on its window). DragOver fires continuously, so
    // keep it to the cheap format check; the real work happens in OnWindowDrop.
    private void OnWindowDragOver(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() != null && e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            e.AcceptedOperation = DataPackageOperation.Copy;
            // Null for some shell drags; the caption is just a cursor hint.
            if (e.DragUIOverride is not null)
            {
                e.DragUIOverride.Caption = Loc.Chrome("import.drop_caption");
            }
        }
        else
        {
            e.AcceptedOperation = DataPackageOperation.None;
        }
    }

    // Scan a dropped folder and open the import dialog on its candidates. Mirrors
    // the macOS window drop: the first dropped folder is scanned with clearFirst,
    // candidates stream into the import store, and the dialog (bound to that list)
    // shows them. Scanning runs off the UI thread; errors surface in the banner.
    private async void OnWindowDrop(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.StorageItems))
        {
            return;
        }

        string? folderPath = null;
        string? readError = null;
        var deferral = e.GetDeferral();
        try
        {
            var items = await e.DataView.GetStorageItemsAsync();
            // Match macOS: the first dropped item must be a folder.
            if (items.FirstOrDefault() is StorageFolder folder && !string.IsNullOrEmpty(folder.Path))
            {
                folderPath = folder.Path;
            }
        }
        catch (Exception)
        {
            readError = Loc.Chrome("import.drop_read_failed");
        }
        finally
        {
            // Release the drop as soon as its data is read — before scanning or
            // showing the dialog — so the drag source isn't left hanging.
            deferral.Complete();
        }

        if (readError is not null)
        {
            ShowImportBanner(readError);
            return;
        }

        if (folderPath is null)
        {
            ShowImportBanner(Loc.Chrome("import.drop_folder_only"));
            return;
        }

        await ImportFolder(folderPath);
    }

    // Scan a folder and open the import dialog on its candidates — candidates
    // stream into the import store and the dialog (bound to that list) shows
    // them, on a scan error too, matching macOS, which navigates to import
    // regardless of the scan result. Shared by the window drop target and a
    // folder activation intent (the folder verb or bae://import); the caller
    // has already confirmed a library is open.
    private async System.Threading.Tasks.Task ImportFolder(string folderPath)
    {
        var (current, error) = await _import.ScanFolder(folderPath);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            ShowImportBanner(error);
        }

        // Skip if one is already open (only one ContentDialog can open at a time).
        if (!_importDialog.IsOpen)
        {
            await _importDialog.Show();
        }
    }

    private void ShowImportBanner(string message)
    {
        _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("import.error_title"), message);
    }

    private void OnQueueClick(object sender, RoutedEventArgs e)
    {
        _queuePane.Toggle();
    }

    // Start a drag from an album card: carry the album ids as the newline-joined
    // payload the queue pane decodes — the whole multi-selection (visible order)
    // when the pressed card is part of it, else just that card. Cancelled when no
    // library is open. Never mutates the selection.
    private void OnAlbumDragStarting(object sender, DragItemsStartingEventArgs e)
    {
        if (CurrentHandleOrNull() == null)
        {
            e.Cancel = true;
            return;
        }
        var pressed = e.Items.OfType<Album>().FirstOrDefault();
        var ids = pressed is null
            ? e.Items.OfType<Album>().Select(album => album.Id).ToList()
            : _albumSelection.OrderedTargets(pressed.Id, AlbumPosition);
        e.Data.SetText(QueueDragPayload.Encode(ids));
        e.Data.RequestedOperation = DataPackageOperation.Copy;
    }

    // The queue button is also an append drop target: a card dropped on it adds
    // the album's tracks to the end of the manual lane, and the +N badge animates
    // from core's queue-items-added event.
    private void OnQueueButtonDragOver(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.Text))
        {
            return;
        }
        e.AcceptedOperation = DataPackageOperation.Copy;
        if (e.DragUIOverride is not null)
        {
            e.DragUIOverride.Caption = Loc.Chrome("menu.add_to_queue");
        }
        e.Handled = true;
    }

    private async void OnQueueButtonDrop(object sender, DragEventArgs e)
    {
        if (CurrentHandleOrNull() == null || !e.DataView.Contains(StandardDataFormats.Text))
        {
            return;
        }
        e.Handled = true;
        await _queuePane.HandleButtonAppendDrop(e);
    }

    private void OnPlayPause(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.PlayPause);
        }
    }

    private void OnNext(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.Next);
        }
    }

    private void OnPrevious(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.Previous);
        }
    }

    private void OnRepeat(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(handle => NativeBae.SetRepeatMode(handle, _playback.RepeatMode.Next()));
        }
    }

    private void OnMute(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(handle => NativeBae.SetMuted(handle, !_playback.IsMuted));
        }
    }

    // Ctrl+F focuses the search box from anywhere in the window.
    private void OnFocusSearchAccelerator(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        SearchBox.Focus(FocusState.Programmatic);
        args.Handled = true;
    }

    // Ctrl+L jumps to whatever's playing: open its album's detail and scroll the
    // track into view, flashing it. No-op when nothing is playing.
    private async void OnGoToNowPlaying(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        await OpenNowPlayingAlbum();
    }

    // Ctrl+1..9 switch to the Nth discovered library. No-op when the digit is
    // beyond the list or already the active library; open failures land on the
    // existing status text and unlock dialog, like any other switch.
    private async void OnLibrarySwitchAccelerator(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        var digit = (int)sender.Key - (int)VirtualKey.Number0;
        var libraries = LoadLibraries();
        var target = LibrarySwitchModel.TargetLibraryId(
            libraries.ConvertAll(library => (library.Id, library.IsActive)), digit);
        if (target is not null)
        {
            await SwitchLibrary(target);
        }
    }

    // Clicking the bar's album art or track title jumps to the playing album —
    // the pointer version of the go-to-now-playing accelerator.
    private async void OnNowPlayingInfoTapped(object sender, TappedRoutedEventArgs e)
    {
        e.Handled = true;
        await OpenNowPlayingAlbum();
    }

    // Open the playing track's album and scroll it into view. No-op when nothing
    // is playing.
    private async System.Threading.Tasks.Task OpenNowPlayingAlbum()
    {
        var albumId = _playback.NowPlayingAlbumId;
        if (CurrentHandleOrNull() == null || string.IsNullOrEmpty(albumId))
        {
            return;
        }

        await _albumDetail.Show(albumId, scrollToTrackId: _playback.NowPlayingTrackId);
    }

    // Space toggles play/pause from anywhere — except while typing in a text
    // field, where space must insert a space. Handled here, not as a button
    // accelerator, so a bare Space key never steals input from a text box.
    // Dialog/flyout text inputs are safe for free: they live in separate popups,
    // not under this root Grid, so their KeyDown never bubbles here. The focus
    // check only has to cover text inputs in the main tree — the search box and
    // the welcome chooser's restore-code box.
    private void OnGlobalKeyDown(object sender, KeyRoutedEventArgs e)
    {
        // Escape closes the queue pane when it's open. Dialogs capture their own
        // input layer, so their Escape never reaches this root handler. The
        // queue pane keeps priority: the album-grid selection only clears once
        // it isn't open.
        if (e.Key == VirtualKey.Escape && _queuePane.IsOpen)
        {
            _queuePane.Hide();
            e.Handled = true;
            return;
        }

        if (e.Key == VirtualKey.Escape && AlbumGrid.Visibility == Visibility.Visible && !_albumSelection.IsEmpty)
        {
            _albumSelection.Clear();
            SyncAlbumSelectionTint();
            e.Handled = true;
            return;
        }

        var focused = FocusManager.GetFocusedElement(Content.XamlRoot);
        var focusedTextInput = focused is TextBox || focused is AutoSuggestBox;

        // Ctrl+A selects every loaded album, guarded by the same focused-text-input
        // check as Space so Ctrl+A in the search box still selects its text.
        if (e.Key == VirtualKey.A && !focusedTextInput
            && AlbumGrid.Visibility == Visibility.Visible && IsModifierDown(VirtualKey.Control))
        {
            _albumSelection.SelectAll(Albums.Select(album => album.Id).ToList());
            SyncAlbumSelectionTint();
            e.Handled = true;
            return;
        }

        if (e.Key != VirtualKey.Space || focusedTextInput)
        {
            return;
        }

        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.PlayPause);
            e.Handled = true;
        }
    }

    private void OnVolumeChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        _nowPlayingBar.HandleVolumeSliderChanged();
    }

    private void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var query = args.QueryText?.Trim() ?? string.Empty;
        if (query.Length == 0)
        {
            LoadCurrentBrowserMode();
        }
        else
        {
            RenderSearch(_browser.Search(query));
        }
    }

    // Show the search pane and render the store's cover-attached results; a session
    // closed mid-search leaves the current view in place.
    private void RenderSearch(BrowserSearch search)
    {
        if (search.HandleGone)
        {
            return;
        }

        // The album grid leaves the screen for the search results pane.
        _albumSelection.Clear();
        SyncAlbumSelectionTint();
        ShowSearchBrowser();
        _browserPanes.RenderSearchResults(search.Results, search.Error);
    }

    private async void OnComposerClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not ComposerSummary composer)
        {
            return;
        }

        await _browserPanes.ShowComposerDetail(composer.ArtistId);
    }

    private async void OnArtistClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not ArtistSummary artist)
        {
            return;
        }

        await _browserPanes.ShowArtistDetail(artist.ArtistId);
    }

    // Dispatch by the modifiers held at click time: Ctrl toggles the clicked
    // album, Shift extends the range from the anchor, and a plain click clears
    // the multi-selection and opens the detail dialog. Modifier clicks never
    // open the dialog.
    private async void OnAlbumClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not Album album)
        {
            return;
        }

        if (IsModifierDown(VirtualKey.Control))
        {
            _albumSelection.Toggle(album.Id);
            SyncAlbumSelectionTint();
            return;
        }
        if (IsModifierDown(VirtualKey.Shift))
        {
            _albumSelection.ExtendRange(album.Id, AlbumPosition, AlbumIdAt);
            SyncAlbumSelectionTint();
            return;
        }

        _albumSelection.Clear();
        SyncAlbumSelectionTint();
        await _albumDetail.Show(album.Id);
    }

    // Right-click / long-press on an album card: the bulk-action menu for
    // whatever the card targets — the whole multi-selection (visible order)
    // for a member card, else just that card. Never mutates the selection.
    private void OnAlbumGridRightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null
            || (e.OriginalSource as FrameworkElement)?.DataContext is not Album album)
        {
            return;
        }

        var targets = _albumSelection.OrderedTargets(album.Id, AlbumPosition);
        var menu = targets.Count == 1
            ? BuildSingleAlbumCardMenu(album)
            : BuildBulkAlbumCardMenu(targets);
        if (menu is null)
        {
            return;
        }

        e.Handled = true;
        var element = (FrameworkElement)e.OriginalSource;
        menu.ShowAt(element, new FlyoutShowOptions { Position = e.GetPosition(element) });
    }

    // The pre-existing single-album menu: release-based actions on the card's
    // primary release. Null when the album carries none (defensive — every
    // grid-loaded album has one; only a search-result album wouldn't).
    private MenuFlyout? BuildSingleAlbumCardMenu(Album album)
    {
        var releaseId = album.PrimaryReleaseId;
        if (string.IsNullOrEmpty(releaseId))
        {
            return null;
        }
        return AlbumCardMenu.Build(
            targetCount: 1,
            onPlay: () =>
            {
                WithCurrentHandle(handle => NativeBae.PlayRelease(handle, releaseId, -1, false));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPlayNext: () =>
            {
                WithCurrentHandle(handle => NativeBae.AddReleaseNext(handle, releaseId));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onAddToQueue: () =>
            {
                WithCurrentHandle(handle => NativeBae.AddReleaseToQueue(handle, releaseId));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPin: () => PinReleases(new[] { releaseId }));
    }

    // The bulk menu for a multi-selected card: batch actions over every
    // targeted album, in visible grid order.
    private MenuFlyout BuildBulkAlbumCardMenu(IReadOnlyList<string> targets)
    {
        var primaryReleaseIds = PrimaryReleaseIds(targets);
        return AlbumCardMenu.Build(
            targetCount: targets.Count,
            onPlay: () =>
            {
                WithCurrentHandle(handle => NativeBae.PlayReleases(handle, primaryReleaseIds));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPlayNext: () => _queuePane.AddAlbumsToQueue(targets, addNext: true),
            onAddToQueue: () => _queuePane.AddAlbumsToQueue(targets, addNext: false),
            onPin: () => PinReleases(primaryReleaseIds));
    }

    // Enqueue releases to pin for offline, surfacing a failure through the
    // shell error banner. Runs off the UI thread: the pin enqueue awaits core's
    // async library-manager call.
    private async System.Threading.Tasks.Task PinReleases(IReadOnlyList<string> releaseIds)
    {
        var (current, error) = await _session.RunForCurrentHandle(handle => releaseIds.Count == 1
            ? NativeBae.PinRelease(handle, releaseIds[0])
            : NativeBae.PinReleases(handle, releaseIds));
        if (current && error is not null)
        {
            _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("error.title"), error);
        }
    }

    // The targeted albums' primary release ids, in target order, dropping any
    // target with none (a search-result album would have none; grid albums
    // always do).
    private List<string> PrimaryReleaseIds(IReadOnlyList<string> albumIds)
    {
        var albumsById = Albums.ToDictionary(album => album.Id);
        return albumIds
            .Select(id => albumsById.TryGetValue(id, out var album) ? album.PrimaryReleaseId : null)
            .Where(releaseId => !string.IsNullOrEmpty(releaseId))
            .Select(releaseId => releaseId!)
            .ToList();
    }

    // The clicked album's index in the loaded grid, or null if it isn't loaded —
    // the position delegate AlbumGridSelectionModel needs for range-extend and
    // ordered targets.
    private int? AlbumPosition(string id)
    {
        for (var i = 0; i < Albums.Count; i++)
        {
            if (Albums[i].Id == id)
            {
                return i;
            }
        }
        return null;
    }

    private string? AlbumIdAt(int index) =>
        index >= 0 && index < Albums.Count ? Albums[index].Id : null;

    // Sync every loaded album's tint from the selection model. Called after
    // every mutation; O(loaded count), which is at most the first page (500).
    private void SyncAlbumSelectionTint()
    {
        foreach (var album in Albums)
        {
            album.IsSelected = _albumSelection.Contains(album.Id);
        }
    }

    private static bool IsModifierDown(VirtualKey key) =>
        Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(key)
            .HasFlag(global::Windows.UI.Core.CoreVirtualKeyStates.Down);

    private async void OnStorageClick(object sender, RoutedEventArgs e)
    {
        await _storageDialog.Show();
    }

    private async void OnSettingsClick(object sender, RoutedEventArgs e)
    {
        await _settingsDialog.Show();
    }

    // Place the window at its saved bounds and maximized state. Wrapped whole:
    // placement must never take down launch, so any failure logs and leaves the
    // system's default placement.
    private void RestoreWindowBounds()
    {
        try
        {
            var plan = WindowBoundsModel.PlanRestore(WindowBoundsStore.Load(), WorkAreasPrimaryFirst());
            if (plan is null)
            {
                return;
            }

            AppWindow.MoveAndResize(new RectInt32(
                plan.Bounds.X, plan.Bounds.Y, plan.Bounds.Width, plan.Bounds.Height));

            // Apply the bounds first, then maximize, so AppWindow records the sane
            // normal bounds as its restore-down target.
            if (plan.Maximized && AppWindow.Presenter is OverlappedPresenter presenter)
            {
                presenter.Maximize();
            }
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not restore the saved window bounds.", exception);
        }
    }

    // The current work areas, with the primary display first so a saved rect that
    // overlaps no display falls back to it.
    private static List<PixelRect> WorkAreasPrimaryFirst()
    {
        var workAreas = new List<PixelRect>();
        var primary = DisplayArea.Primary;
        if (primary is not null)
        {
            workAreas.Add(ToPixelRect(primary.WorkArea));
        }

        foreach (var display in DisplayArea.FindAll())
        {
            if (primary is null || display.DisplayId.Value != primary.DisplayId.Value)
            {
                workAreas.Add(ToPixelRect(display.WorkArea));
            }
        }

        return workAreas;
    }

    private static PixelRect ToPixelRect(RectInt32 rect) =>
        new(rect.X, rect.Y, rect.Width, rect.Height);

    // Track the window's normal bounds and maximized state as it moves and
    // resizes. A field assignment per event, no I/O: OnClosed does the single
    // write. A minimized window carries no bounds worth saving, so it leaves the
    // last-seen normal bounds and maximized flag untouched.
    private void OnAppWindowChanged(AppWindow sender, AppWindowChangedEventArgs args)
    {
        if (sender.Presenter is not OverlappedPresenter presenter
            || presenter.State == OverlappedPresenterState.Minimized)
        {
            return;
        }

        _maximized = presenter.State == OverlappedPresenterState.Maximized;

        if ((args.DidPositionChange || args.DidSizeChange)
            && presenter.State == OverlappedPresenterState.Restored)
        {
            _lastNormalBounds = new PixelRect(
                sender.Position.X, sender.Position.Y, sender.Size.Width, sender.Size.Height);
        }
    }

    private async void OnClosed(object sender, WindowEventArgs args)
    {
        if (!_closeTeardownDone && CurrentHandleOrNull() != null)
        {
            // First pass: cancel this close so the async teardown below can't
            // race process exit — `async void` would otherwise let the window
            // die at the first `await`, before the shutdown's
            // persist_playback_state write lands. Run the teardown
            // deterministically, then close again; the re-entrant call below
            // takes the fast path (no live handle) and lets the close
            // proceed.
            args.Handled = true;

            // Clear the transport controls first so no ghost entry lingers
            // during shutdown; OnClosed doesn't go through TearDownLibrary.
            // Idempotent.
            _mediaControls.Deactivate();
            // Always shut down gracefully; the restore-on-launch preference
            // gates the restore at the next launch (passed to InitApp), not
            // this save — the core keeps the resume row current either way.
            await ShutdownAndFreeCurrentHandle();
            SaveWindowBounds();

            _closeTeardownDone = true;
            Close();
            return;
        }

        // Re-entry after the teardown above, or a close with no library ever
        // opened: nothing left to await, so let the close proceed. Bounds are
        // only saved here if the first pass didn't already run above.
        if (!_closeTeardownDone)
        {
            SaveWindowBounds();
        }

        BaeDiagnostics.Flush();
    }

    // Persist the last-seen normal bounds and maximized state, if the window
    // ever settled into a restored position, so the next launch reopens here.
    private void SaveWindowBounds()
    {
        if (_lastNormalBounds is PixelRect normalBounds)
        {
            WindowBoundsStore.Save(WindowBoundsModel.Serialize(normalBounds, _maximized));
        }
    }
}
