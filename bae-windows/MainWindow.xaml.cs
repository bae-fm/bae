using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
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
    private bool _populatingSortBox;

    // The library session (handle + event subscription) and the stores it drives.
    private readonly SessionStore _session;
    private readonly LibraryBrowserStore _browser;
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
    private readonly QueueDialog _queueDialog;
    private readonly StorageStore _storage;
    private readonly StorageDialog _storageDialog;

    // Drives the system media transport controls (hardware media keys + the
    // Windows media flyout) from playback events. One instance for the window's
    // lifetime; library switches deactivate it rather than recreating it.
    private readonly MediaControlService _mediaControls;

    // Re-reads generated bridge settings into the open settings dialog's labels; set while that
    // dialog is open so config invalidations refresh it live, null when closed.
    private Action? _refreshSettings;

    // Drives the settings "updates" section: a launch-time background check and
    // the manual check/download/apply from settings. Inert on a dev run or a
    // loose-zip copy (IsAvailable is false).
    private readonly UpdateService _updateService = new();

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
        PopulateSortBox();
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
            NpRemaining,
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
            text => StatusText.Text = text);
        _albumDetail = new AlbumDetailDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
            _releaseActions);
        _queueDialog = new QueueDialog(_session, _playback, () => Content.XamlRoot);

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
        _projections.Register(typeof(BridgeInvalidation.Config), () => _refreshSettings?.Invoke());
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

    private void OnSortChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_populatingSortBox)
        {
            return;
        }
        if (SortBox.SelectedIndex < 0)
        {
            return;
        }

        _browser.Sort.SetActive(_browser.Sort.OptionAt(SortBox.SelectedIndex));

        // Sort drives the full-library view; search results keep their relevance
        // order. Reload only when no search is active and the library is open.
        if (CurrentHandleOrNull() != null && string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private void OnBrowserModeChanged(object sender, SelectionChangedEventArgs e)
    {
        _browser.Sort.SetMode(BrowserModeBox.SelectedIndex == 1 ? BrowserMode.Composers : BrowserMode.Albums);
        PopulateSortBox();
        if (CurrentHandleOrNull() != null && string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private void PopulateSortBox()
    {
        _populatingSortBox = true;
        SortBox.Items.Clear();
        foreach (var option in _browser.Sort.ActiveOptions)
        {
            SortBox.Items.Add(Loc.Chrome(option.LabelKey));
        }
        SortBox.SelectedIndex = _browser.Sort.ActiveIndex;
        _populatingSortBox = false;
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

    private System.Threading.Tasks.Task<(bool Current, T Result)>
        RunForCurrentHandle<T>(Func<AppHandle, T> action) =>
        _session.RunForCurrentHandle(action);

    private System.Threading.Tasks.Task<bool> RunForCurrentHandle(Action<AppHandle> action) =>
        _session.RunForCurrentHandle(action);

    private bool WithCurrentHandle(Action<AppHandle> action) =>
        _session.WithCurrentHandle(action);

    private (bool Current, T Result) WithCurrentHandle<T>(Func<AppHandle, T> action) =>
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

    private async void OnQueueClick(object sender, RoutedEventArgs e)
    {
        await _queueDialog.Show();
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

    private async void OnStorageClick(object sender, RoutedEventArgs e)
    {
        await _storageDialog.Show();
    }

    private async void OnSettingsClick(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var (current, s) = WithCurrentHandle(NativeBae.GetSettings);
        if (!current)
        {
            return;
        }

        // Discogs key state machine. The token input is the only local draft state;
        // the configured/valid state comes from generated bridge settings, re-read on
        // a config invalidation. not_configured/rejected → editable input + Save; valid →
        // "connected" + Remove; unvalidated → that label + Re-check + Remove. Save
        // and Re-check validate over the network, so they run off the UI thread and
        // show "Validating…" while in flight.
        //
        // Two text lines: `status` is the persisted state (driven only by
        // RenderDiscogs from the settings re-read, plus the in-flight "Validating…");
        // `settingsErrorText` is local feedback for an action — a rejected key,
        // a settings write failure, a re-check / remove failure — cleared when
        // the next action starts. Keeping them apart means an unrelated
        // a config-invalidation re-render can't wipe the rejection note.
        var status = new TextBlock { TextWrapping = TextWrapping.Wrap, Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray) };
        var settingsErrorText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        var tokenBox = new TextBox { PlaceholderText = Loc.Chrome("settings.discogs.token_placeholder") };
        var save = new Button { Content = Loc.Chrome("settings.discogs.save") };
        var recheck = new Button { Content = Loc.Chrome("settings.discogs.recheck") };
        var remove = new Button { Content = Loc.Chrome("settings.discogs.remove") };
        var discogsBusy = false;

        void ShowSettingsError(string message)
        {
            settingsErrorText.Text = message;
            settingsErrorText.Visibility = Visibility.Visible;
        }

        void ClearSettingsError()
        {
            settingsErrorText.Text = string.Empty;
            settingsErrorText.Visibility = Visibility.Collapsed;
        }

        // Drive the controls from the persisted status: which buttons show, whether
        // the input is editable, and the status line. Called on open and on every
        // config-invalidation re-read. The draft text and the local error line are left
        // alone — they belong to the user's in-progress input, not the stored state.
        void RenderDiscogs(Settings settings)
        {
            if (discogsBusy)
            {
                return;
            }

            tokenBox.Visibility = settings.DiscogsConfigured ? Visibility.Collapsed : Visibility.Visible;
            save.Visibility = settings.DiscogsConfigured ? Visibility.Collapsed : Visibility.Visible;
            remove.Visibility = settings.DiscogsConfigured ? Visibility.Visible : Visibility.Collapsed;
            recheck.Visibility = settings.DiscogsNeedsRecheck ? Visibility.Visible : Visibility.Collapsed;
            status.Text = settings.DiscogsConfigured ? settings.DiscogsStatusText : string.Empty;
        }

        save.Click += async (_, _) =>
        {
            var token = tokenBox.Text ?? string.Empty;
            if (string.IsNullOrEmpty(token) || discogsBusy)
            {
                return;
            }

            discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, outcome) = await RunForCurrentHandle(
                handle => NativeBae.SaveDiscogsToken(handle, token));
            discogsBusy = false;
            if (!current)
            {
                return;
            }
            switch (outcome)
            {
                case "valid":
                case "unvalidated":
                    // Stored: a config-invalidation re-read settles the controls and label.
                    status.Text = string.Empty;
                    break;
                case "rejected":
                    // Nothing stored, so no config invalidation fires — keep the draft and
                    // surface the rejection.
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.rejected"));
                    break;
                default:
                    status.Text = string.Empty;
                    ShowSettingsError(Loc.Chrome("settings.discogs.save_failed"));
                    break;
            }
        };
        recheck.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            discogsBusy = true;
            ClearSettingsError();
            status.Text = Loc.Chrome("settings.discogs.validating");
            var (current, error) = await RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);
            discogsBusy = false;
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // On success a config-invalidation re-read settles the controls and label.
        };
        remove.Click += async (_, _) =>
        {
            if (discogsBusy)
            {
                return;
            }

            ClearSettingsError();
            // Removing clears the config flag, firing a config invalidation — the re-read
            // restores the editable input. Nothing is patched inline here.
            var (current, error) = await RunForCurrentHandle(NativeBae.DeleteDiscogsToken);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
        };

        var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        buttons.Children.Add(save);
        buttons.Children.Add(recheck);
        buttons.Children.Add(remove);

        var syncStatus = new TextBlock { Text = s.SyncStatusText };
        // Two-step disconnect: the first click surfaces the data-loss warning (when
        // releases live only in the cloud) inline and arms; the second confirms.
        // A nested ContentDialog can't open over the settings dialog.
        var disconnect = new Button { Content = Loc.Chrome("settings.sync.disconnect") };
        var disconnectArmed = false;
        disconnect.Click += async (_, _) =>
        {
            if (!disconnectArmed)
            {
                var (warningCurrent, warning) = await RunForCurrentHandle(NativeBae.DisconnectWarning);
                if (!warningCurrent)
                {
                    return;
                }
                if (warning is not null)
                {
                    syncStatus.Text = Loc.Chrome("settings.sync.disconnect_confirm", "warning", warning);
                    disconnectArmed = true;
                    return;
                }
            }

            disconnectArmed = false;
            var (disconnectCurrent, error) = WithCurrentHandle(NativeBae.DisconnectCloud);
            if (!disconnectCurrent)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _refreshSettings?.Invoke();
            }
        };
        var syncNow = new Button { Content = Loc.Chrome("settings.sync.now") };
        syncNow.Click += (_, _) => WithCurrentHandle(NativeBae.TriggerSync);
        var syncButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        syncButtons.Children.Add(disconnect);
        syncButtons.Children.Add(syncNow);

        // Opaque (encrypted) vs browsable (stored in the clear), applied to
        // whichever provider is connected below. Defaults to the secure choice.
        // Not access control — the bucket's own credentials gate it either way.
        var storagePicker = new ComboBox { Header = Loc.Chrome("settings.storage.mode"), SelectedIndex = 0 };
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.opaque"), Tag = "opaque" });
        storagePicker.Items.Add(new ComboBoxItem { Content = Loc.Chrome("settings.storage.browsable"), Tag = "browsable" });
        string SelectedStorage() =>
            (storagePicker.SelectedItem as ComboBoxItem)?.Tag as string ?? "opaque";

        // OAuth providers: signing in runs the browser flow in the core, so it
        // blocks until the user finishes — run it off the UI thread.
        Button CloudButton(string label, string provider)
        {
            var button = new Button { Content = label };
            button.Click += async (_, _) =>
            {
                if (!OAuthCreds.Available)
                {
                    syncStatus.Text = OAuthCreds.RegistrationError
                        ?? Loc.Chrome("cloud.signin.not_configured");
                    return;
                }
                syncStatus.Text = Loc.Chrome("cloud.signin.in_progress", "provider", label);
                var storage = SelectedStorage();
                var (current, error) = await RunForCurrentHandle(
                    handle => NativeBae.SignInCloud(handle, provider, storage));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    syncStatus.Text = error;
                }
                else
                {
                    _refreshSettings?.Invoke();
                }
            };
            return button;
        }

        // Only offer the OAuth providers this build's native library supports.
        // An S3-only build returns just S3, so no sign-in button renders.
        var available = NativeBae.AvailableCloudProviders();
        var oauthButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (var wire in new[] { "google_drive", "dropbox", "onedrive" })
        {
            if (available.Contains(wire))
            {
                oauthButtons.Children.Add(CloudButton(BridgeDisplay.ProviderDisplayName(wire), wire));
            }
        }

        var content = new StackPanel { Spacing = 8, MinWidth = 360 };
        var libraryLabel = new TextBlock { Text = Loc.Chrome("settings.library_label", "name", s.LibraryName) };
        var pauseBetweenSides = new CheckBox
        {
            Content = Loc.Chrome("settings.playback.pause_between_sides"),
            IsChecked = s.PauseBetweenSides,
        };
        var refreshingSettings = false;
        async System.Threading.Tasks.Task SetPauseBetweenSides(bool enabled)
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetPauseBetweenSides(handle, enabled));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                refreshingSettings = true;
                pauseBetweenSides.IsChecked = !enabled;
                refreshingSettings = false;
            }
        }

        pauseBetweenSides.Checked += async (_, _) => await SetPauseBetweenSides(true);
        pauseBetweenSides.Unchecked += async (_, _) => await SetPauseBetweenSides(false);

        // Export: the single-track "Save As…" suggested-filename template and the
        // metadata tags an export embeds. Windows has no release export, so this is
        // the per-track section only. Writes round-trip through config invalidation into
        // the settings re-read (RenderExport); the checkboxes send the whole seven-
        // bool set (set-state), never one mutated field.
        var exportLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.export.label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        var exportTemplate = new TextBox
        {
            Header = Loc.Chrome("settings.export.filename_format"),
            Text = s.ExportFilenameTemplate,
        };
        var exportTokensHelp = new TextBlock
        {
            Text = Loc.Chrome("settings.export.tokens_help"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        var defaultTrackExport = new ComboBox { Header = Loc.Chrome("settings.export.default_track_format") };
        var defaultReleaseExport = new ComboBox { Header = Loc.Chrome("settings.export.default_release_format") };
        var presetPanel = new StackPanel { Spacing = 8 };
        var addPresetButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var addFlacPreset = new Button { Content = Loc.Chrome("settings.export.add_flac") };
        var addMp3Preset = new Button { Content = Loc.Chrome("settings.export.add_mp3") };
        var addOpusPreset = new Button { Content = Loc.Chrome("settings.export.add_opus") };
        var addWavPreset = new Button { Content = Loc.Chrome("settings.export.add_wav") };
        var addAiffPreset = new Button { Content = Loc.Chrome("settings.export.add_aiff") };
        addPresetButtons.Children.Add(addFlacPreset);
        addPresetButtons.Children.Add(addMp3Preset);
        addPresetButtons.Children.Add(addOpusPreset);
        addPresetButtons.Children.Add(addWavPreset);
        addPresetButtons.Children.Add(addAiffPreset);

        void RenderExport(Settings settings)
        {
            if (refreshingSettings)
            {
                return;
            }

            refreshingSettings = true;
            exportTemplate.Text = settings.ExportFilenameTemplate;
            PopulateExportSelection(defaultTrackExport, settings, release: false);
            PopulateExportSelection(defaultReleaseExport, settings, release: true);
            RenderExportPresets(settings);
            refreshingSettings = false;
        }

        void PopulateExportSelection(ComboBox combo, Settings settings, bool release)
        {
            combo.Items.Clear();
            var original = ExportSelection.Original();
            var selected = release ? settings.DefaultReleaseExportSelection : settings.DefaultTrackExportSelection;
            combo.Items.Add(new ComboBoxItem
            {
                Content = Loc.Chrome("track.export.original"),
                Tag = original,
                IsSelected = SameExportSelection(selected, original),
            });
            foreach (var preset in settings.ExportPresets.Where(p => release ? p.AppliesToRelease : p.AppliesToTrack))
            {
                var selection = ExportSelection.Preset(preset.Id);
                combo.Items.Add(new ComboBoxItem
                {
                    Content = preset.Name,
                    Tag = selection,
                    IsSelected = SameExportSelection(selected, selection),
                });
            }
        }

        bool SameExportSelection(BridgeExportSelection a, BridgeExportSelection b) =>
            ExportSelection.Equal(a, b);

        string CodecLabel(BridgeExportPresetCodec codec) => codec switch
        {
            BridgeExportPresetCodec.Flac => "FLAC",
            BridgeExportPresetCodec.Mp3 mp3 => $"MP3 {mp3.BitrateKbps} kbps",
            BridgeExportPresetCodec.OpusOgg opus => $"Opus {opus.BitrateKbps} kbps",
            BridgeExportPresetCodec.Wav => "WAV",
            BridgeExportPresetCodec.Aiff => "AIFF",
            _ => string.Empty,
        };

        void RenderExportPresets(Settings settings)
        {
            presetPanel.Children.Clear();
            foreach (var preset in settings.ExportPresets)
            {
                var name = new TextBox
                {
                    Header = Loc.Chrome("settings.export.preset_name"),
                    Text = preset.Name,
                };
                var filenameTemplate = new TextBox
                {
                    Header = Loc.Chrome("settings.export.filename_format"),
                    Text = preset.FilenameTemplate,
                };
                var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                var codec = new TextBlock
                {
                    Text = CodecLabel(preset.Codec),
                    VerticalAlignment = VerticalAlignment.Center,
                    MinWidth = 120,
                };
                var track = new CheckBox
                {
                    Content = Loc.Chrome("settings.export.preset_track"),
                    IsChecked = preset.AppliesToTrack,
                };
                var release = new CheckBox
                {
                    Content = Loc.Chrome("settings.export.preset_release"),
                    IsChecked = preset.AppliesToRelease,
                };
                row.Children.Add(codec);
                row.Children.Add(track);
                row.Children.Add(release);
                var codecEditor = BuildPresetCodecEditor(preset);

                var pregap = new ComboBox { Header = Loc.Chrome("settings.export.preset_pregap") };
                foreach (var item in ExportPregapChoices(preset.Codec))
                {
                    pregap.Items.Add(new ComboBoxItem
                    {
                        Content = item.Label,
                        Tag = item.Value,
                        IsSelected = preset.PregapPlacement == item.Value,
                    });
                }
                void ApplyPregapApplicability()
                {
                    var singleFileCue = pregap.SelectedItem is ComboBoxItem selected
                        && selected.Tag is BridgeExportPregapPlacement placement
                        && placement == BridgeExportPregapPlacement.SingleFileWithCue;
                    if (singleFileCue)
                    {
                        track.IsChecked = false;
                        release.IsChecked = true;
                    }
                    track.IsEnabled = !singleFileCue;
                    release.IsEnabled = !singleFileCue;
                }
                pregap.SelectionChanged += (_, _) => ApplyPregapApplicability();
                ApplyPregapApplicability();
                var save = new Button { Content = Loc.Chrome("action.save") };
                var remove = new Button { Content = Loc.Chrome("action.remove") };
                var editor = new StackPanel { Spacing = 6 };
                editor.Children.Add(name);
                editor.Children.Add(filenameTemplate);
                editor.Children.Add(row);
                editor.Children.Add(codecEditor.View);
                editor.Children.Add(pregap);
                var buttons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                buttons.Children.Add(save);
                buttons.Children.Add(remove);
                editor.Children.Add(buttons);
                presetPanel.Children.Add(editor);

                save.Click += async (_, _) =>
                {
                    preset.Name = name.Text ?? string.Empty;
                    if (filenameTemplate.Text is not string template)
                    {
                        ShowSettingsError(Loc.Chrome("settings.export.save_failed"));
                        return;
                    }
                    preset.FilenameTemplate = template;
                    preset.AppliesToTrack = track.IsChecked == true;
                    preset.AppliesToRelease = release.IsChecked == true;
                    if (pregap.SelectedItem is ComboBoxItem selected && selected.Tag is BridgeExportPregapPlacement placement)
                    {
                        preset.PregapPlacement = placement;
                    }
                    codecEditor.Apply();
                    await SaveExportPresets(settings.ExportPresets);
                };
                remove.Click += async (_, _) =>
                {
                    settings.ExportPresets.Remove(preset);
                    await SaveExportPresets(settings.ExportPresets);
                };
            }
        }

        (StackPanel View, Action Apply) BuildPresetCodecEditor(ExportPreset preset)
        {
            var panel = new StackPanel { Spacing = 6 };
            switch (preset.Codec)
            {
                case BridgeExportPresetCodec.Flac:
                case BridgeExportPresetCodec.Wav:
                case BridgeExportPresetCodec.Aiff:
                    var currentBitDepth = preset.Codec switch
                    {
                        BridgeExportPresetCodec.Flac current => current.BitDepth,
                        BridgeExportPresetCodec.Wav current => current.BitDepth,
                        BridgeExportPresetCodec.Aiff current => current.BitDepth,
                        _ => BridgeExportBitDepth.Source,
                    };
                    var bitDepth = new ComboBox { Header = Loc.Chrome("settings.export.bit_depth_label") };
                    foreach (var item in ExportBitDepthChoices())
                    {
                        bitDepth.Items.Add(new ComboBoxItem
                        {
                            Content = item.Label,
                            Tag = item.Value,
                            IsSelected = currentBitDepth == item.Value,
                        });
                    }
                    panel.Children.Add(bitDepth);
                    return (
                        panel,
                        () =>
                        {
                            if (bitDepth.SelectedItem is ComboBoxItem selected && selected.Tag is BridgeExportBitDepth selectedBitDepth)
                            {
                                preset.Codec = preset.Codec switch
                                {
                                    BridgeExportPresetCodec.Flac => new BridgeExportPresetCodec.Flac(selectedBitDepth),
                                    BridgeExportPresetCodec.Wav => new BridgeExportPresetCodec.Wav(selectedBitDepth),
                                    BridgeExportPresetCodec.Aiff => new BridgeExportPresetCodec.Aiff(selectedBitDepth),
                                    _ => preset.Codec,
                                };
                            }
                        }
                    );
                case BridgeExportPresetCodec.Mp3:
                case BridgeExportPresetCodec.OpusOgg:
                    var currentBitrate = preset.Codec switch
                    {
                        BridgeExportPresetCodec.Mp3 current => current.BitrateKbps,
                        BridgeExportPresetCodec.OpusOgg current => current.BitrateKbps,
                        _ => 0u,
                    };
                    var bitrate = new TextBox
                    {
                        Header = Loc.Chrome("settings.export.bitrate"),
                        Text = currentBitrate.ToString(CultureInfo.InvariantCulture),
                    };
                    panel.Children.Add(bitrate);
                    return (
                        panel,
                        () =>
                        {
                            if (uint.TryParse(bitrate.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var bitrateKbps))
                            {
                                preset.Codec = preset.Codec switch
                                {
                                    BridgeExportPresetCodec.Mp3 => new BridgeExportPresetCodec.Mp3(bitrateKbps),
                                    BridgeExportPresetCodec.OpusOgg => new BridgeExportPresetCodec.OpusOgg(bitrateKbps),
                                    _ => preset.Codec,
                                };
                                return;
                            }
                            preset.Codec = preset.Codec switch
                            {
                                BridgeExportPresetCodec.Mp3 => new BridgeExportPresetCodec.Mp3(0),
                                BridgeExportPresetCodec.OpusOgg => new BridgeExportPresetCodec.OpusOgg(0),
                                _ => preset.Codec,
                            };
                        }
                    );
                default:
                    return (panel, () => { });
            }
        }

        List<(string Label, BridgeExportPregapPlacement Value)> ExportPregapChoices(BridgeExportPresetCodec codec)
        {
            var choices = new List<(string Label, BridgeExportPregapPlacement Value)>
            {
                (
                    Loc.Chrome("settings.export.pregap.append_except_htoa"),
                    BridgeExportPregapPlacement.AppendToPreviousExceptHtoa
                ),
                (
                    Loc.Chrome("settings.export.pregap.append_including_htoa"),
                    BridgeExportPregapPlacement.AppendToPreviousIncludingHtoa
                ),
                (Loc.Chrome("settings.export.pregap.exclude"), BridgeExportPregapPlacement.Exclude),
            };

            if (ExportCodecSupportsSingleFileCue(codec))
            {
                choices.Add((
                    Loc.Chrome("settings.export.pregap.single_file_with_cue"),
                    BridgeExportPregapPlacement.SingleFileWithCue
                ));
            }

            return choices;
        }

        static bool ExportCodecSupportsSingleFileCue(BridgeExportPresetCodec codec) =>
            codec is not BridgeExportPresetCodec.OpusOgg;

        List<(string Label, BridgeExportBitDepth Value)> ExportBitDepthChoices() => new()
        {
            (Loc.Chrome("settings.export.bit_depth.source"), BridgeExportBitDepth.Source),
            (Loc.Chrome("settings.export.bit_depth.bits16"), BridgeExportBitDepth.Bits16),
            (Loc.Chrome("settings.export.bit_depth.bits24"), BridgeExportBitDepth.Bits24),
            (Loc.Chrome("settings.export.bit_depth.bits32"), BridgeExportBitDepth.Bits32),
        };

        async System.Threading.Tasks.Task SaveExportTemplate()
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var template = exportTemplate.Text ?? string.Empty;
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetExportFilenameTemplate(handle, template));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
            }
            // On success a config-invalidation re-read settles the field via RenderExport.
        }

        async System.Threading.Tasks.Task SaveExportPresets(List<ExportPreset> presets)
        {
            if (refreshingSettings)
            {
                return;
            }

            ClearSettingsError();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetExportPresets(handle, presets));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _refreshSettings?.Invoke();
            }
        }

        async System.Threading.Tasks.Task SaveDefaultExportSelection(ComboBox combo, bool release)
        {
            if (refreshingSettings || combo.SelectedItem is not ComboBoxItem item || item.Tag is not BridgeExportSelection selection)
            {
                return;
            }

            ClearSettingsError();
            var (current, error) = await RunForCurrentHandle(handle => release
                ? NativeBae.SetDefaultReleaseExportSelection(handle, selection)
                : NativeBae.SetDefaultTrackExportSelection(handle, selection));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                _refreshSettings?.Invoke();
            }
        }

        ExportPreset MakeExportPreset(string kind)
        {
            BridgeExportPresetCodec codec = kind switch
            {
                "mp3" => new BridgeExportPresetCodec.Mp3(320),
                "opus_ogg" => new BridgeExportPresetCodec.OpusOgg(192),
                "wav" => new BridgeExportPresetCodec.Wav(BridgeExportBitDepth.Source),
                "aiff" => new BridgeExportPresetCodec.Aiff(BridgeExportBitDepth.Source),
                _ => new BridgeExportPresetCodec.Flac(BridgeExportBitDepth.Source),
            };
            var extension = kind switch
            {
                "mp3" => "mp3",
                "opus_ogg" => "ogg",
                "wav" => "wav",
                "aiff" => "aiff",
                _ => "flac",
            };
            var label = kind switch
            {
                "mp3" => "MP3",
                "opus_ogg" => "Opus",
                "wav" => "WAV",
                "aiff" => "AIFF",
                _ => "FLAC",
            };
            return new ExportPreset
            {
                Id = Guid.NewGuid().ToString("N"),
                Name = label,
                Codec = codec,
                Extension = extension,
                FilenameTemplate = exportTemplate.Text ?? string.Empty,
                PregapPlacement = BridgeExportPregapPlacement.AppendToPreviousExceptHtoa,
                AppliesToTrack = true,
                AppliesToRelease = true,
            };
        }

        exportTemplate.LostFocus += async (_, _) => await SaveExportTemplate();
        exportTemplate.KeyDown += async (_, args) =>
        {
            if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                await SaveExportTemplate();
            }
        };
        defaultTrackExport.SelectionChanged += async (_, _) =>
            await SaveDefaultExportSelection(defaultTrackExport, release: false);
        defaultReleaseExport.SelectionChanged += async (_, _) =>
            await SaveDefaultExportSelection(defaultReleaseExport, release: true);
        addFlacPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("flac"));
            await SaveExportPresets(s.ExportPresets);
        };
        addMp3Preset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("mp3"));
            await SaveExportPresets(s.ExportPresets);
        };
        addOpusPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("opus_ogg"));
            await SaveExportPresets(s.ExportPresets);
        };
        addWavPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("wav"));
            await SaveExportPresets(s.ExportPresets);
        };
        addAiffPreset.Click += async (_, _) =>
        {
            s.ExportPresets.Add(MakeExportPreset("aiff"));
            await SaveExportPresets(s.ExportPresets);
        };

        var automationLabel = new TextBlock
        {
            Text = Loc.Chrome("settings.automation.label"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        };
        var mcpEnabled = new CheckBox
        {
            Content = Loc.Chrome("settings.automation.enable_mcp"),
            IsChecked = s.McpEnabled,
        };
        var mcpPort = new TextBox
        {
            Header = Loc.Chrome("settings.automation.port"),
            Text = s.McpPort.ToString(CultureInfo.InvariantCulture),
        };
        var mcpStatus = new TextBlock
        {
            Text = s.McpStatusText,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        };
        var saveMcp = new Button { Content = Loc.Chrome("action.save") };
        var refreshMcp = new Button { Content = Loc.Chrome("settings.automation.refresh") };
        var copyMcpToken = new Button { Content = Loc.Chrome("settings.automation.copy_token") };
        var rotateMcpToken = new Button { Content = Loc.Chrome("settings.automation.rotate_token") };
        var mcpButtons = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        mcpButtons.Children.Add(saveMcp);
        mcpButtons.Children.Add(refreshMcp);
        mcpButtons.Children.Add(copyMcpToken);
        mcpButtons.Children.Add(rotateMcpToken);

        void RenderMcp(Settings settings)
        {
            if (refreshingSettings)
            {
                return;
            }

            refreshingSettings = true;
            mcpEnabled.IsChecked = settings.McpEnabled;
            mcpPort.Text = settings.McpPort.ToString(CultureInfo.InvariantCulture);
            mcpStatus.Text = settings.McpStatusText;
            refreshingSettings = false;
        }

        async System.Threading.Tasks.Task SetMcpConfig(bool enabled)
        {
            if (refreshingSettings)
            {
                return;
            }

            if (!ushort.TryParse(mcpPort.Text, NumberStyles.None, CultureInfo.InvariantCulture, out var port) || port == 0)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.invalid_port"));
                return;
            }

            ClearSettingsError();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetMcpServerConfig(handle, enabled, port));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                refreshingSettings = true;
                mcpEnabled.IsChecked = !enabled;
                refreshingSettings = false;
                return;
            }
            _refreshSettings?.Invoke();
        }

        async System.Threading.Tasks.Task RefreshMcpStatus()
        {
            var (current, status) = await RunForCurrentHandle(NativeBae.McpServerStatus);
            if (!current)
            {
                return;
            }
            mcpStatus.Text = Settings.McpStatusTextFor(status);
        }

        async System.Threading.Tasks.Task CopyMcpToken(Func<AppHandle, string?> readToken, string successKey)
        {
            ClearSettingsError();
            var (current, token) = await RunForCurrentHandle(readToken);
            if (!current)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            ClipboardHelper.CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome(successKey);
        }

        mcpEnabled.Checked += async (_, _) => await SetMcpConfig(true);
        mcpEnabled.Unchecked += async (_, _) => await SetMcpConfig(false);
        saveMcp.Click += async (_, _) => await SetMcpConfig(mcpEnabled.IsChecked == true);
        refreshMcp.Click += async (_, _) => await RefreshMcpStatus();
        copyMcpToken.Click += async (_, _) =>
            await CopyMcpToken(NativeBae.GetMcpToken, "settings.automation.token_copied");
        rotateMcpToken.Click += async (_, _) =>
        {
            ClearSettingsError();
            var (tokenCurrent, token) = await RunForCurrentHandle(NativeBae.GenerateMcpToken);
            if (!tokenCurrent)
            {
                return;
            }
            if (token is null)
            {
                ShowSettingsError(Loc.Chrome("settings.automation.token_unavailable"));
                return;
            }
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SetMcpToken(handle, token));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                ShowSettingsError(error);
                return;
            }
            ClipboardHelper.CopyToClipboard(token);
            mcpStatus.Text = Loc.Chrome("settings.automation.token_rotated");
        };

        var discogsLabel = new TextBlock { Text = Loc.Chrome("settings.discogs.label") };
        content.Children.Add(libraryLabel);
        content.Children.Add(pauseBetweenSides);
        content.Children.Add(exportLabel);
        content.Children.Add(exportTemplate);
        content.Children.Add(exportTokensHelp);
        content.Children.Add(defaultTrackExport);
        content.Children.Add(defaultReleaseExport);
        content.Children.Add(new TextBlock { Text = Loc.Chrome("settings.export.presets"), FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        content.Children.Add(addPresetButtons);
        content.Children.Add(presetPanel);
        content.Children.Add(automationLabel);
        content.Children.Add(mcpEnabled);
        content.Children.Add(mcpPort);
        content.Children.Add(mcpButtons);
        content.Children.Add(mcpStatus);
        content.Children.Add(discogsLabel);
        content.Children.Add(tokenBox);
        content.Children.Add(buttons);
        content.Children.Add(status);
        content.Children.Add(settingsErrorText);
        RenderDiscogs(s);
        // S3-compatible provider form. The core probes the bucket before saving.
        var s3Bucket = new TextBox { Header = Loc.Chrome("s3.field.bucket") };
        var s3Region = new TextBox { Header = Loc.Chrome("s3.field.region") };
        var s3Endpoint = new TextBox { Header = Loc.Chrome("s3.field.endpoint") };
        var s3KeyPrefix = new TextBox { Header = Loc.Chrome("s3.field.key_prefix") };
        var s3AccessKey = new TextBox { Header = Loc.Chrome("s3.field.access_key") };
        var s3SecretKey = new PasswordBox { Header = Loc.Chrome("s3.field.secret_key") };
        var connectS3 = new Button { Content = Loc.Chrome("settings.s3.connect") };
        connectS3.Click += async (_, _) =>
        {
            syncStatus.Text = Loc.Chrome("settings.s3.connecting");
            var storage = SelectedStorage();
            var (current, error) = await RunForCurrentHandle(
                handle => NativeBae.SaveSyncConfig(
                    handle,
                    s3Bucket.Text ?? string.Empty,
                    s3Region.Text ?? string.Empty,
                    s3Endpoint.Text ?? string.Empty,
                    s3KeyPrefix.Text ?? string.Empty,
                    s3AccessKey.Text ?? string.Empty,
                    s3SecretKey.Password ?? string.Empty,
                    storage));
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                syncStatus.Text = error;
            }
            else
            {
                _refreshSettings?.Invoke();
            }
        };
        var s3Form = new StackPanel { Spacing = 6 };
        s3Form.Children.Add(s3Bucket);
        s3Form.Children.Add(s3Region);
        s3Form.Children.Add(s3Endpoint);
        s3Form.Children.Add(s3KeyPrefix);
        s3Form.Children.Add(s3AccessKey);
        s3Form.Children.Add(s3SecretKey);
        s3Form.Children.Add(connectS3);

        content.Children.Add(syncStatus);
        content.Children.Add(syncButtons);
        content.Children.Add(storagePicker);
        content.Children.Add(oauthButtons);
        content.Children.Add(s3Form);

        // Devices (membership): list the library's devices with their role and a
        // "this device" marker. The owner can add a device (which opens the
        // approve flow) or remove one (which rotates the library key). The list
        // loads off the UI thread; the add-device button only renders for an owner.
        var addDeviceRequested = false;
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("members.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var membersHost = new StackPanel { Spacing = 8 };
        membersHost.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        content.Children.Add(membersHost);

        // Recovery: the restore code is now a recovery secret only — it restores
        // this library on a new device when there's no other device available to
        // approve it. Anyone with it has full access, so it's revealed on demand,
        // behind a warning, never shown by default.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.recovery.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.recovery.intro"),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        var recoveryCode = new TextBox
        {
            Header = Loc.Chrome("settings.recovery.label"),
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            Visibility = Visibility.Collapsed,
        };
        var showRecoveryCode = new Button { Content = Loc.Chrome("settings.recovery.show") };
        showRecoveryCode.Click += async (_, _) =>
        {
            var (current, code) = await RunForCurrentHandle(NativeBae.GenerateRestoreCode);
            if (!current)
            {
                return;
            }
            recoveryCode.Text = code ?? Loc.Chrome("settings.recovery.unavailable");
            recoveryCode.Visibility = Visibility.Visible;
        };
        content.Children.Add(showRecoveryCode);
        content.Children.Add(recoveryCode);

        // Updates: the installed version, and — for a Velopack install — a manual
        // check that downloads in the background and applies on restart. A dev run
        // or a loose-zip copy is not an install, so only the version line shows.
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.updates.title"),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        var installedVersion = _updateService.InstalledVersion is { } version
            ? UpdateFlowDisplay.VersionDisplay(version)
            : AppMetadata.ConfiguredString("BaeGitCommit");
        content.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("settings.updates.version", "version", installedVersion),
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });

        var restartUpdateRequested = false;
        Button? updateRestartButton = null;
        Action? unsubscribeUpdates = null;
        if (_updateService.IsAvailable)
        {
            var updateStatus = new TextBlock
            {
                TextWrapping = TextWrapping.Wrap,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            };
            var checkUpdates = new Button { Content = Loc.Chrome("settings.updates.check") };
            var restartUpdate = new Button { Content = Loc.Chrome("settings.updates.restart") };
            updateRestartButton = restartUpdate;

            void RenderUpdates(UpdateFlowState state)
            {
                if (UpdateFlowDisplay.StatusFor(state) is { } mapped)
                {
                    updateStatus.Text = mapped.Args is { } args
                        ? Loc.Chrome(mapped.Key, args)
                        : Loc.Chrome(mapped.Key);
                    updateStatus.Visibility = Visibility.Visible;
                }
                else
                {
                    updateStatus.Text = string.Empty;
                    updateStatus.Visibility = Visibility.Collapsed;
                }
                checkUpdates.IsEnabled = UpdateFlowDisplay.CheckEnabled(state);
                restartUpdate.Visibility = UpdateFlowDisplay.RestartVisible(state)
                    ? Visibility.Visible
                    : Visibility.Collapsed;
            }

            checkUpdates.Click += async (_, _) => await _updateService.CheckAsync();

            // State transitions arrive on a worker thread; marshal to the UI
            // thread. Subscribed for the dialog's lifetime, so reopening settings
            // reflects a background download that finished while it was closed
            // (the phase lives on the service, not the dialog).
            void OnUpdateStateChanged(UpdateFlowState state) =>
                DispatcherQueue.TryEnqueue(() => RenderUpdates(state));
            _updateService.StateChanged += OnUpdateStateChanged;
            unsubscribeUpdates = () => _updateService.StateChanged -= OnUpdateStateChanged;

            RenderUpdates(_updateService.State);
            content.Children.Add(updateStatus);
            content.Children.Add(checkUpdates);
            content.Children.Add(restartUpdate);
        }

        // Lock this library: forget its encryption key on this device. Sync stops
        // and the library reopens to the unlock prompt; local files stay.
        var lockRequested = false;
        var lockButton = new Button { Content = Loc.Chrome("settings.lock_library") };
        content.Children.Add(lockButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("settings.title"),
            Content = new ScrollViewer { Content = content },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        lockButton.Click += (_, _) =>
        {
            lockRequested = true;
            dialog.Hide();
        };

        // The restart button applies a staged update: hide the dialog and run the
        // apply after ShowAsync returns (a nested dialog can't open over this one,
        // same as the lock dance). Only wired when the updates section rendered it.
        if (updateRestartButton is not null)
        {
            updateRestartButton.Click += (_, _) =>
            {
                restartUpdateRequested = true;
                dialog.Hide();
            };
        }

        // Now that the dialog exists, load the device list into its placeholder.
        // The add-device button (owner-only) arms the approve flow and closes the
        // settings dialog — a nested ContentDialog can't open over it, so the
        // approve flow runs after this one returns (mirroring the lock dance).
        _ = LoadMembersInto(membersHost, () =>
        {
            addDeviceRequested = true;
            dialog.Hide();
        });

        // Re-read the (generated bridge-pre-computed) settings into the live labels so a
        // config invalidation — or a connect/disconnect in this dialog — updates
        // them in place instead of requiring a reopen.
        _refreshSettings = () =>
        {
            var (current, fresh) = WithCurrentHandle(NativeBae.GetSettings);
            if (!current)
            {
                return;
            }

            syncStatus.Text = fresh.SyncStatusText;
            libraryLabel.Text = Loc.Chrome("settings.library_label", "name", fresh.LibraryName);
            refreshingSettings = true;
            pauseBetweenSides.IsChecked = fresh.PauseBetweenSides;
            refreshingSettings = false;
            RenderExport(fresh);
            RenderMcp(fresh);
            RenderDiscogs(fresh);
        };

        // A key saved while offline lands "unvalidated"; opening settings is a
        // chance to settle it now that there may be connectivity. The core no-ops
        // unless the stored key is actually unvalidated, so call unconditionally;
        // on a result it changes the status, firing a config invalidation.
        _ = RunForCurrentHandle(NativeBae.RevalidateDiscogsToken);

        await dialog.ShowAsync();
        _refreshSettings = null;
        unsubscribeUpdates?.Invoke();

        if (restartUpdateRequested)
        {
            // ApplyUpdatesAndRestart exits the process, so persist playback state
            // and flush diagnostics first — the work OnClosed would otherwise do.
            await ShutdownAndFreeCurrentHandle();
            BaeDiagnostics.Flush();
            _updateService.ApplyAndRestart();
            return;
        }

        if (lockRequested)
        {
            var (lockCurrent, error) = await RunForCurrentHandle(NativeBae.LockActiveLibrary);
            if (!lockCurrent)
            {
                return;
            }
            if (error is not null)
            {
                StatusText.Text = error;
                return;
            }

            // The key is forgotten now, so re-opening lands on the unlock prompt.
            await ShutdownAndFreeCurrentHandle();

            OpenLibrary(s.LibraryId);
            return;
        }

        // Add-a-device closed settings to open the approve flow (no nested
        // dialogs). Run it, then reopen settings so the refreshed device list
        // shows the newly-approved device.
        if (addDeviceRequested)
        {
            await _approveDialog.Show();
            OnSettingsClick(sender, e);
        }
    }

    // Load the library's devices into a host panel: one row per device (short
    // fingerprint + role + "this device" marker), and — for an owner — an
    // "Add a device…" button plus a Remove control on each other device. Runs the
    // blocking generated bridge off the UI thread. <paramref name="onAddDevice"/> arms the
    // approve flow (which the caller runs once the settings dialog closes).
    private async System.Threading.Tasks.Task LoadMembersInto(StackPanel host, Action onAddDevice)
    {
        var (current, result) = await RunForCurrentHandle(NativeBae.GetMembers);
        if (!current)
        {
            return;
        }
        host.Children.Clear();

        if (result.Error is not null)
        {
            host.Children.Add(new TextBlock
            {
                Text = result.Error,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        var membership = result.Membership;
        if (membership is null)
        {
            host.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.load_failed"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
            });
            return;
        }

        foreach (var member in membership.Members)
        {
            host.Children.Add(BuildMemberRow(member, host, onAddDevice));
        }

        if (membership.SelfIsOwner)
        {
            var add = new Button { Content = Loc.Chrome("members.add") };
            add.Click += (_, _) => onAddDevice();
            host.Children.Add(add);
        }
    }

    // One device row: fingerprint + role badge + "this device" marker, plus a
    // two-step Remove for the owner on every other device. Removing rotates the
    // library key, so it confirms inline (a second click) — a nested ContentDialog
    // can't open over the settings dialog.
    private FrameworkElement BuildMemberRow(BridgeMember member, StackPanel host, Action onAddDevice)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };

        var labels = new StackPanel { Spacing = 0 };
        labels.Children.Add(new TextBlock
        {
            Text = member.Fingerprint,
            FontFamily = new FontFamily("Consolas"),
        });
        if (member.IsSelf)
        {
            labels.Children.Add(new TextBlock
            {
                Text = Loc.Chrome("members.this_device"),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
        }
        row.Children.Add(labels);

        row.Children.Add(new TextBlock
        {
            Text = MemberFormat.RoleLabel(member.Role),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            VerticalAlignment = VerticalAlignment.Center,
        });

        // The owner can remove any device but its own.
        if (member.CanRemove)
        {
            var remove = new Button { Content = Loc.Chrome("members.remove") };
            var status = new TextBlock
            {
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            var armed = false;
            remove.Click += async (_, _) =>
            {
                if (!armed)
                {
                    status.Text = Loc.Chrome("members.remove_confirm");
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
                    status.Visibility = Visibility.Visible;
                    armed = true;
                    return;
                }

                remove.IsEnabled = false;
                var pubkey = member.Pubkey;
                var (current, error) = await RunForCurrentHandle(
                    handle => NativeBae.RemoveMember(handle, pubkey));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                    status.Visibility = Visibility.Visible;
                    remove.IsEnabled = true;
                    armed = false;
                    return;
                }

                // Reload the list in place so the removed device disappears.
                await LoadMembersInto(host, onAddDevice);
            };
            row.Children.Add(remove);

            var rowWithStatus = new StackPanel { Spacing = 4 };
            rowWithStatus.Children.Add(row);
            rowWithStatus.Children.Add(status);
            return rowWithStatus;
        }

        return row;
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
        BaeDiagnostics.Flush();
    }
}
