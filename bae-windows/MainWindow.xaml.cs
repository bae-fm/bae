using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
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
    private readonly UiEventRouter _router;
    private readonly NowPlayingBarController _nowPlayingBar;
    private readonly ImportStore _import;
    private readonly ImportDialog _importDialog;
    private readonly ImportPickerDialog _importPicker;
    private readonly ImportConfirmDialog _importConfirm;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly AlbumDetailDialog _albumDetail;
    private readonly QueuePane _queuePane;
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

    // x:Bind Albums/Composers in the shell resolve here; the collections live in
    // the browser store (constructed before InitializeComponent so these are
    // non-null when the bindings first evaluate).
    public ObservableCollection<Album> Albums => _browser.Albums;
    public ObservableCollection<ComposerSummary> Composers => _browser.Composers;

    public MainWindow()
    {
        // The browser store owns the Albums/Composers collections the shell binds
        // to with x:Bind, so it (and the session it reads) must exist before
        // InitializeComponent evaluates those bindings.
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
        _router = new UiEventRouter(_playback, _shell, _projections, _mediaControls, _import.HandlePreviewEvent);
        _session.UiEvent += _router.Route;

        // Album detail and the per-release action dialogs it opens, plus the queue
        // dialog. Album detail is the shared entry point from the grid, the panes,
        // the now-playing jump, and the import "view in library" banner.
        _releaseActions = new ReleaseActionDialogs(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
            _projections);
        _albumDetail = new AlbumDetailDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
            _releaseActions);
        _queuePane = new QueuePane(
            _session,
            _playback,
            QueuePaneHost,
            message => _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("error.playback_title"), message));

        // The storage sheet and its non-UI operations. The dialog registers its
        // live-refresh handlers on the projection registry while open.
        _storage = new StorageStore(_session);
        _storageDialog = new StorageDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            _storage,
            _projections);

        // The composer/search panes and the library-lifecycle dialogs. The window
        // stays the navigation shell (open/close/switch); these render and drive
        // their own screens, calling back for the operations it owns.
        _browserPanes = new BrowserPanes(
            _session,
            DispatcherQueue,
            SearchResultsPanel,
            ComposerDetailPane,
            text => StatusText.Text = text,
            ShowComposerBrowser,
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
            _session, () => Content.XamlRoot, albumId => _albumDetail.Show(albumId));
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
            OpenLibrary);

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
        _browser.Sort.SetMode(BrowserModeBox.SelectedIndex == 1 ? BrowserMode.Composers : BrowserMode.Albums);
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
        SearchResultsScroll.Visibility = Visibility.Collapsed;
    }

    private void ShowComposerBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Visible;
        SearchResultsScroll.Visibility = Visibility.Collapsed;
    }

    private void ShowSearchBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        SearchResultsScroll.Visibility = Visibility.Visible;
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

        var load = _browser.LoadAlbums();
        if (load.Result == BrowserLoadResult.HandleGone)
        {
            return;
        }
        ShowAlbumBrowser();
        RenderGridStatus(load);
    }

    // Set the status line from a completed grid load; a handle-gone load leaves it
    // untouched. Visibility is the caller's concern.
    private void RenderGridStatus(BrowserGridLoad load)
    {
        switch (load.Result)
        {
            case BrowserLoadResult.HandleGone:
                return;
            case BrowserLoadResult.Failed:
                StatusText.Text = load.Error ?? Loc.Chrome("library.load_failed");
                return;
            default:
                StatusText.Text = load.IsEmpty
                    ? Loc.Chrome(load.Mode == BrowserMode.Composers ? "library.no_composers" : "library.empty")
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
                return;
            case OpenHandleResult.NeedsUnlock:
                // Encrypted library whose key isn't on this device: the session
                // freed the handle it made; prompt for the key rather than show a
                // half-open library. Unlocking re-opens with sync online.
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
        SearchBox.Text = string.Empty;
        StatusText.Text = string.Empty;
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

        var (current, error) = await _import.ScanFolder(folderPath);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            ShowImportBanner(error);
        }

        // Open the import dialog on the streamed candidates — on scan error too,
        // matching macOS, which navigates to import regardless of the scan result.
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
    // payload the queue pane decodes. Cancelled when no library is open.
    private void OnAlbumDragStarting(object sender, DragItemsStartingEventArgs e)
    {
        if (CurrentHandleOrNull() == null)
        {
            e.Cancel = true;
            return;
        }
        var ids = e.Items.OfType<Album>().Select(album => album.Id).ToList();
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
            WithCurrentHandle(NativeBae.CycleRepeatMode);
        }
    }

    private void OnMute(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.ToggleMute);
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
        // input layer, so their Escape never reaches this root handler.
        if (e.Key == VirtualKey.Escape && _queuePane.IsOpen)
        {
            _queuePane.Hide();
            e.Handled = true;
            return;
        }

        if (e.Key != VirtualKey.Space)
        {
            return;
        }

        var focused = FocusManager.GetFocusedElement(Content.XamlRoot);
        if (focused is TextBox || focused is AutoSuggestBox)
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

    private async void OnAlbumClick(object sender, ItemClickEventArgs e)
    {
        if (CurrentHandleOrNull() == null || e.ClickedItem is not Album album)
        {
            return;
        }

        await _albumDetail.Show(album.Id);
    }

    // Right-click / long-press on an album card: play or queue the album's
    // canonical release without opening the detail dialog.
    private void OnAlbumGridRightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null
            || (e.OriginalSource as FrameworkElement)?.DataContext is not Album album)
        {
            return;
        }
        var releaseId = album.PrimaryReleaseId;
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }
        e.Handled = true;
        var element = (FrameworkElement)e.OriginalSource;
        var menu = AlbumCardMenu.Build(
            onPlay: () => WithCurrentHandle(handle => NativeBae.PlayRelease(handle, releaseId, -1, false)),
            onPlayNext: () => WithCurrentHandle(handle => NativeBae.AddReleaseNext(handle, releaseId)),
            onAddToQueue: () => WithCurrentHandle(handle => NativeBae.AddReleaseToQueue(handle, releaseId)));
        menu.ShowAt(element, new FlyoutShowOptions { Position = e.GetPosition(element) });
    }

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
        // Clear the transport controls first so no ghost entry lingers during
        // shutdown; OnClosed doesn't go through TearDownLibrary. Idempotent.
        _mediaControls.Deactivate();
        if (CurrentHandleOrNull() != null)
        {
            // Persist the queue / current track / position before freeing the
            // handle, so the next launch can restore where playback left off.
            await ShutdownAndFreeCurrentHandle();
        }

        // Persist the last-seen normal bounds and maximized state, if the window
        // ever settled into a restored position, so the next launch reopens here.
        if (_lastNormalBounds is PixelRect normalBounds)
        {
            WindowBoundsStore.Save(WindowBoundsModel.Serialize(normalBounds, _maximized));
        }

        BaeDiagnostics.Flush();
    }
}
