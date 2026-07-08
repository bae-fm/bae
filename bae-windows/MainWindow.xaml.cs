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

    // Releases whose unmanage is running right now. Unmanage is a blocking
    // foreground transfer (unlike pin, which enqueues, or upload, which lives in
    // the outbox), so it has no queue snapshot to read — we track it here while
    // RunStorageActionForReleases awaits, letting the storage row offer to cancel
    // it. UI-thread only (added/removed around the await, read when building the
    // menu).
    private readonly HashSet<string> _unmanagingReleases = new();

    // Drives the system media transport controls (hardware media keys + the
    // Windows media flyout) from playback events. One instance for the window's
    // lifetime; library switches deactivate it rather than recreating it.
    private readonly MediaControlService _mediaControls;

    // Reloads the storage dialog's outbox panel and storage rows; set while that
    // dialog is open so outbox invalidations refresh them live, null when closed.
    private Action? _refreshOutbox;
    private Action? _refreshDownloads;

    // Re-reads generated bridge settings into the open settings dialog's labels; set while that
    // dialog is open so config invalidations refresh it live, null when closed.
    private Action? _refreshSettings;

    // Drives the settings "updates" section: a launch-time background check and
    // the manual check/download/apply from settings. Inert on a dev run or a
    // loose-zip copy (IsAvailable is false).
    private readonly UpdateService _updateService = new();

    // Reloads the storage dialog's release rows; set while that dialog is open so
    // album/release invalidations refresh each row's state badge and actions, not
    // just the album grid.
    private Action? _refreshStorageRows;

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
        _projections.Register(typeof(BridgeInvalidation.Album), () => _refreshStorageRows?.Invoke());
        _projections.Register(typeof(BridgeInvalidation.Release), () => _refreshStorageRows?.Invoke());
        _projections.Register(typeof(BridgeInvalidation.Config), () => _refreshSettings?.Invoke());
        _projections.Register(typeof(BridgeInvalidation.SyncStatus), _sync.Refresh);
        _projections.Register(typeof(BridgeInvalidation.Outbox), () => _refreshOutbox?.Invoke());
        _projections.Register(typeof(BridgeInvalidation.DownloadQueue), () => _refreshDownloads?.Invoke());
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
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        var listPanel = new StackPanel { Spacing = 4, MinWidth = 460 };
        var storageStatus = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // The releases whose rows are selected. Right-clicking applies the
        // chosen action to the whole selection (or to just the right-tapped row
        // when it isn't part of it). Releases that vanish on reload (e.g. an
        // local release moved out of the library) drop out below.
        var selected = new HashSet<string>();
        // The current rows, kept so the right-tap menu can resolve a release's
        // allowed actions (for the multi-select intersection) by id.
        var rowsById = new Dictionary<string, BridgeStorageRow>();

        // Each row shows its summary; a left-click toggles its selection and a
        // right-click opens a menu of the transitions the core says it allows
        // (carried on the row, gated on cloud-home + pending uploads), plus
        // cancel for any queued uploads. The same actions run on every selected
        // release.
        async System.Threading.Tasks.Task LoadStorageRows()
        {
            var (current, result) = await RunForCurrentHandle(NativeBae.StorageRows);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            if (result.Rows is null)
            {
                storageStatus.Text = Loc.Chrome("storage.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            storageStatus.Visibility = Visibility.Collapsed;
            rowsById.Clear();
            foreach (var row in result.Rows)
            {
                rowsById[row.Release.Id] = row;
            }
            // Drop selections for releases no longer present after a transition.
            selected.IntersectWith(rowsById.Keys);

            listPanel.Children.Clear();
            foreach (var row in result.Rows)
            {
                var text = new TextBlock
                {
                    Text = StorageRowSummary(row),
                    VerticalAlignment = VerticalAlignment.Center,
                    TextWrapping = TextWrapping.Wrap,
                };
                var releaseId = row.Release.Id;
                var rowBorder = new Border
                {
                    Child = text,
                    // The release id rides on Tag so RefreshRowHighlights can
                    // recolor each row from the current selection.
                    Tag = releaseId,
                    Padding = new Thickness(6, 4, 6, 4),
                    CornerRadius = new CornerRadius(4),
                    Background = RowBackground(selected.Contains(releaseId)),
                };

                rowBorder.Tapped += (_, _) =>
                {
                    if (!selected.Add(releaseId))
                    {
                        selected.Remove(releaseId);
                    }
                    rowBorder.Background = RowBackground(selected.Contains(releaseId));
                };
                rowBorder.RightTapped += async (_, args) =>
                {
                    // The args are only valid synchronously; capture the tap
                    // position before any await.
                    var position = args.GetPosition(rowBorder);

                    // Act on the selection when this row is part of it, else on
                    // just this row (and select it, matching the macOS menu).
                    if (!selected.Contains(releaseId))
                    {
                        selected.Clear();
                        selected.Add(releaseId);
                        RefreshRowHighlights();
                    }

                    var menu = await BuildStorageRowMenu(
                        selected.ToList(), rowsById, storageStatus, LoadStorageRows);
                    // Nothing to offer (e.g. no cloud home, or uploads in flight)
                    // — skip the empty popup.
                    if (menu.Items.Count > 0)
                    {
                        menu.ShowAt(rowBorder, new FlyoutShowOptions { Position = position });
                    }
                };

                listPanel.Children.Add(rowBorder);
            }
        }

        // Repaint every row's background from the current selection. The storage
        // list is a flat StackPanel of Borders tagged with their release id.
        void RefreshRowHighlights()
        {
            foreach (var child in listPanel.Children)
            {
                if (child is Border border && border.Tag is string id)
                {
                    border.Background = RowBackground(selected.Contains(id));
                }
            }
        }

        await LoadStorageRows();

        // Cloud outbox: the upload/delete queue with a summary band, a Retry-now
        // button, and per-item Cancel. Hidden (empty panel) when nothing is queued.
        // Reloaded after retry/cancel so the panel reflects the new queue state.
        var downloadsPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadDownloads()
        {
            downloadsPanel.Children.Clear();
            var (current, result) = await RunForCurrentHandle(NativeBae.DownloadSnapshot);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            var snapshot = result.Snapshot;
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("storage.read_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            // Hidden when the pin queue is idle, like the outbox panel.
            if (snapshot.Downloads.Length == 0)
            {
                return;
            }

            string StateLabel(BridgeDownloadOp op) => op.State switch
            {
                BridgeDownloadState.Active => Loc.Chrome("download.state.downloading"),
                BridgeDownloadState.Failed => Loc.Chrome("download.state.failed"),
                _ => Loc.Chrome("download.state.queued"),
            };

            string DownloadDetail(BridgeDownloadOp op)
            {
                static string DisplayBytes(ulong bytes) =>
                    Loc.Bytes(checked((long)bytes));

                var parts = new List<string>
                {
                    Loc.Chrome("storage.files", "count", op.FileCount),
                    Loc.Bytes(op.TotalSize),
                    StateLabel(op),
                };
                if (DownloadProgress(op.State) is { } progress)
                {
                    parts.Add(
                        Loc.Core(
                            "core.download.bytes_progress",
                            new Dictionary<string, object?>
                            {
                                ["done"] = DisplayBytes(progress.BytesDone),
                                ["total"] = DisplayBytes(progress.BytesTotal),
                            }));
                }
                return string.Join(" · ", parts);
            }

            // Header: a label (or "paused"), Retry (only with failures), and a
            // pause/resume toggle — mirroring the outbox panel's band.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = snapshot.Paused ? Loc.Chrome("download.paused") : Loc.Chrome("download.title"),
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button
            {
                Content = Loc.Chrome("outbox.retry_now"),
                IsEnabled = snapshot.Total.Failed > 0,
            };
            retry.Click += async (_, _) =>
            {
                retry.IsEnabled = false;
                await RunForCurrentHandle(NativeBae.RetryDownloads);
                await LoadDownloads();
            };
            band.Children.Add(retry);
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await RunForCurrentHandle(
                    handle => NativeBae.SetDownloadsPaused(handle, !paused));
                await LoadDownloads();
            };
            band.Children.Add(pause);
            downloadsPanel.Children.Add(band);

            // One row per release: title, "N files · size · state", and a cancel.
            foreach (var op in snapshot.Downloads)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = op.Title, TextWrapping = TextWrapping.Wrap });
                labelColumn.Children.Add(new TextBlock
                {
                    Text = DownloadDetail(op),
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                });
                if (DownloadProgress(op.State) is { } progress)
                {
                    labelColumn.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = 1,
                        Value = progress.Fraction,
                        Height = 4,
                    });
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                var releaseId = op.ReleaseId;
                var cancel = new Button { Content = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    storageStatus.Visibility = Visibility.Collapsed;
                    cancel.IsEnabled = false;
                    var (cancelCurrent, error) = await RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!cancelCurrent)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
                        cancel.IsEnabled = true;
                        return;
                    }

                    await LoadDownloads();
                };
                Grid.SetColumn(cancel, 1);
                itemGrid.Children.Add(cancel);
                downloadsPanel.Children.Add(itemGrid);
            }
        }

        var outboxPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadOutbox()
        {
            outboxPanel.Children.Clear();
            var (current, result) = await RunForCurrentHandle(NativeBae.OutboxSnapshot);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            var snapshot = result.Snapshot;
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("outbox.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            if (snapshot.UploadGroups.Length == 0 && snapshot.Deletes.Length == 0)
            {
                return;
            }

            // With work queued at least one count is non-zero, so compose the
            // localized queue summary from the generated snapshot counts.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = OutboxSummary(snapshot),
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button { Content = Loc.Chrome("outbox.retry_now") };
            retry.Click += async (_, _) =>
            {
                storageStatus.Visibility = Visibility.Collapsed;
                retry.IsEnabled = false;
                var (retryCurrent, error) = await RunForCurrentHandle(NativeBae.RetryOutbox);
                if (!retryCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                    retry.IsEnabled = true;
                    return;
                }

                await LoadOutbox();
            };
            band.Children.Add(retry);
            // Pause/resume the upload pipeline. Paused leaves items queued but stops
            // the sync cycle from draining them.
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await RunForCurrentHandle(handle => NativeBae.SetSyncPaused(handle, !paused));
                await LoadOutbox();
            };
            band.Children.Add(pause);
            outboxPanel.Children.Add(band);

            // Master progress strip: a byte-progress bar (dimmed while paused) and
            // locale-formatted byte / throughput / ETA labels.
            if (snapshot.Total.BytesTotal > 0)
            {
                outboxPanel.Children.Add(new ProgressBar
                {
                    Minimum = 0,
                    Maximum = checked((long)snapshot.Total.BytesTotal),
                    Value = checked((long)snapshot.Total.BytesDone),
                    Opacity = paused ? 0.4 : 1.0,
                });
                var detail = new List<string>();
                var bytesLabel = OutboxBytesLabel(snapshot);
                if (!string.IsNullOrEmpty(bytesLabel))
                {
                    detail.Add(bytesLabel);
                }
                var throughputLabel = OutboxThroughputLabel(snapshot);
                if (!string.IsNullOrEmpty(throughputLabel))
                {
                    detail.Add(throughputLabel);
                }
                var etaLabel = OutboxEtaLabel(snapshot);
                if (!string.IsNullOrEmpty(etaLabel))
                {
                    detail.Add(etaLabel);
                }
                if (detail.Count > 0)
                {
                    outboxPanel.Children.Add(new TextBlock
                    {
                        Text = string.Join(" · ", detail),
                        FontSize = 12,
                        Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    });
                }
            }

            // A queue row: a label (with an optional progress bar), an optional
            // trailing button, and an optional right-click menu.
            void AddOutboxRow(string label, ProgressBar? progress, Button? trailing, MenuFlyout? contextMenu)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = label, TextWrapping = TextWrapping.Wrap });
                if (progress is not null)
                {
                    labelColumn.Children.Add(progress);
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                if (trailing is not null)
                {
                    Grid.SetColumn(trailing, 1);
                    itemGrid.Children.Add(trailing);
                }
                if (contextMenu is not null)
                {
                    itemGrid.ContextFlyout = contextMenu;
                }
                outboxPanel.Children.Add(itemGrid);
            }

            // Runs `action` off-thread, surfaces any error to the status line, and
            // reloads the panel on success — shared by the row button and menu.
            async System.Threading.Tasks.Task RunCancel(Func<AppHandle, string?> action)
            {
                storageStatus.Visibility = Visibility.Collapsed;
                var (current, error) = await RunForCurrentHandle(action);
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                    return;
                }

                await LoadOutbox();
            }

            // A right-click "Cancel" menu, matching the storage table's per-release
            // cancel. Used for the upload release rows.
            MenuFlyout CancelFlyout(Func<AppHandle, string?> action)
            {
                var menu = new MenuFlyout();
                var item = new MenuFlyoutItem { Text = Loc.Chrome("action.cancel") };
                item.Click += async (_, _) => await RunCancel(action);
                menu.Items.Add(item);
                return menu;
            }

            // Uploads: one row per release (matching the storage table) — title,
            // aggregate progress, and a right-click "Cancel" that stops the
            // release's transition. The orphaned-files bucket (no release id) has
            // no release to cancel.
            foreach (var group in snapshot.UploadGroups)
            {
                ProgressBar? progress = group.Progress is { Active: > 0, BytesTotal: > 0 }
                    ? new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = checked((long)group.Progress.BytesTotal),
                        Value = checked((long)group.Progress.BytesDone),
                    }
                    : null;
                MenuFlyout? menu = group.ReleaseId is string releaseId
                    ? CancelFlyout(handle => NativeBae.CancelReleaseTransition(handle, releaseId))
                    : null;
                AddOutboxRow(group.DisplayTitle, progress, trailing: null, contextMenu: menu);
            }
            // A pending delete is genuinely a single-file operation, so it keeps
            // its own per-file cancel button.
            foreach (var delete in snapshot.Deletes)
            {
                var cancel = new Button { Content = Loc.Chrome("outbox.cancel_item") };
                var id = delete.Id;
                cancel.Click += async (_, _) => await RunCancel(
                    handle => NativeBae.CancelOutboxItem(handle, id));
                AddOutboxRow(DeleteLabel(delete), null, trailing: cancel, contextMenu: null);
            }
        }

        await LoadDownloads();
        await LoadOutbox();

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(storageStatus);
        content.Children.Add(downloadsPanel);
        content.Children.Add(outboxPanel);
        content.Children.Add(listPanel);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("storage.title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 480 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = Content.XamlRoot,
        };
        // Refresh both the outbox panel and the storage rows live while the dialog
        // is open as uploads/deletes progress. Stops once the dialog closes.
        _refreshOutbox = () =>
        {
            _ = LoadOutbox();
            _ = LoadStorageRows();
        };
        // Refresh the Downloads pane live as pins progress and the storage rows
        // with them (a row's badge/state changes as a pin completes).
        _refreshDownloads = () =>
        {
            _ = LoadDownloads();
            _ = LoadStorageRows();
        };
        // An album/release invalidation that isn't an outbox change can still
        // alter a release's storage state — refresh the rows for it too.
        _refreshStorageRows = () => _ = LoadStorageRows();
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            _refreshOutbox = null;
            _refreshDownloads = null;
            _refreshStorageRows = null;
        }
    }

    // Selected-row highlight: a faint accent tint, or transparent when not
    // selected. Static so LoadStorageRows and RefreshRowHighlights agree.
    private static Brush RowBackground(bool isSelected) =>
        isSelected
            ? new SolidColorBrush(Microsoft.UI.Colors.SteelBlue) { Opacity = 0.25 }
            : new SolidColorBrush(Microsoft.UI.Colors.Transparent);

    // User-facing label for a storage transition, matching the macOS
    // "Storage…" sheet / context menu wording.
    private static string StorageRowSummary(BridgeStorageRow row)
    {
        var format = string.IsNullOrEmpty(row.Release.Format) ? string.Empty : $" · {row.Release.Format}";
        var files = Loc.Chrome("storage.files", "count", row.Release.FileCount);
        return $"{row.Album.Title} — {row.Album.ArtistNames}{format} · {files} · {Loc.Bytes(row.Release.TotalSize)} · {StorageStateLabel(row.Release.StorageState)}{PinIndicator(row.Release)}";
    }

    private static string StorageStateLabel(BridgeReleaseStorageState state) => state switch
    {
        BridgeReleaseStorageState.Remote => Loc.Chrome("storage.state.managed"),
        BridgeReleaseStorageState.Local => Loc.Chrome("storage.state.unmanaged"),
        _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown storage state"),
    };

    private static string PinIndicator(BridgeReleaseSummary release) =>
        release.Pinned ? $" · {Loc.Chrome("storage.pinned")}" : string.Empty;

    private static BridgeDownloadTransferProgress? DownloadProgress(BridgeDownloadState state) =>
        state is BridgeDownloadState.Active active ? active.Progress : null;

    private static string OutboxSummary(BridgeOutboxSnapshot snapshot)
    {
        var parts = new List<string>();
        if (snapshot.Total.Active > 0) parts.Add(Loc.Core("core.queue.uploading", "count", snapshot.Total.Active));
        if (snapshot.Total.Failed > 0) parts.Add(Loc.Core("core.queue.failed", "count", snapshot.Total.Failed));
        if (snapshot.Total.Queued > 0) parts.Add(Loc.Core("core.queue.queued", "count", snapshot.Total.Queued));
        if (snapshot.Deletes.Length > 0)
            parts.Add(Loc.Core("core.outbox.pending_deletes", "count", snapshot.Deletes.Length));
        return string.Join(" · ", parts);
    }

    private static string OutboxThroughputLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.ThroughputBps > 0
            ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(checked((long)snapshot.ThroughputBps)))
            : string.Empty;

    private static string OutboxEtaLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.EtaSeconds is { } seconds
            ? Loc.Core("core.outbox.eta", "duration", Loc.Duration(checked(checked((long)seconds) * 1000)))
            : string.Empty;

    private static string OutboxBytesLabel(BridgeOutboxSnapshot snapshot)
    {
        if (snapshot.Total.BytesTotal == 0) return string.Empty;
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)snapshot.Total.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)snapshot.Total.BytesTotal)),
            });
    }

    private static string DeleteLabel(BridgeDeleteOp delete) =>
        $"{delete.CloudKey} — {Loc.Chrome("outbox.delete.kind")}";

    private static string StorageActionLabel(BridgeReleaseStorageAction action) => action switch
    {
        BridgeReleaseStorageAction.MakeRemote => Loc.Chrome("storage.action.manage"),
        BridgeReleaseStorageAction.MakeLocal => Loc.Chrome("storage.action.unmanage"),
        BridgeReleaseStorageAction.Pin => Loc.Chrome("storage.action.pin"),
        BridgeReleaseStorageAction.Unpin => Loc.Chrome("storage.action.unpin"),
        _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
    };

    // The transitions every release in the selection allows, intersected so the
    // menu only offers actions applicable to all. Order follows the first
    // release's action list (the core's order). The caller suppresses actions
    // when any targeted release has a transition in flight, matching the macOS
    // "Storage…" sheet.
    private static List<BridgeReleaseStorageAction> IntersectedStorageActions(
        List<string> releaseIds, Dictionary<string, BridgeStorageRow> rowsById)
    {
        var perRelease = releaseIds
            .Select(id => rowsById.TryGetValue(id, out var row)
                ? new HashSet<BridgeReleaseStorageAction>(row.Release.StorageActions)
                : new HashSet<BridgeReleaseStorageAction>())
            .ToList();
        if (perRelease.Count == 0)
        {
            return new List<BridgeReleaseStorageAction>();
        }

        var common = perRelease[0];
        foreach (var set in perRelease.Skip(1))
        {
            common.IntersectWith(set);
        }
        // Preserve the core's action order from the first release's row.
        var order = rowsById.TryGetValue(releaseIds[0], out var firstRow)
            ? firstRow.Release.StorageActions
            : [];
        return order.Where(common.Contains).ToList();
    }

    // Build the right-tap menu for the targeted releases: the intersected
    // storage transitions plus a cancel for any of their queued uploads. Each
    // item runs the action on every targeted release, then reloads the rows.
    private async System.Threading.Tasks.Task<MenuFlyout> BuildStorageRowMenu(
        List<string> releaseIds,
        Dictionary<string, BridgeStorageRow> rowsById,
        TextBlock storageStatus,
        Func<System.Threading.Tasks.Task> reload)
    {
        var menu = new MenuFlyout();

        // A release with a transition in flight offers only "Cancel" — the
        // storage actions (pin/unmanage/…) would race it. Each transition
        // surfaces differently: an upload sits in the outbox snapshot, a pin in
        // the download queue snapshot, and an unmanage (a blocking foreground
        // transfer with no queue) is tracked locally while it runs. Core
        // dispatches to whichever is running.
        var transitioning = new HashSet<string>(
            await UploadingReleases(releaseIds, storageStatus));
        transitioning.UnionWith(await DownloadingReleases(releaseIds));
        transitioning.UnionWith(releaseIds.Where(_unmanagingReleases.Contains));
        if (transitioning.Count > 0)
        {
            var cancel = new MenuFlyoutItem { Text = Loc.Chrome("action.cancel") };
            cancel.Click += async (_, _) =>
            {
                foreach (var releaseId in transitioning)
                {
                    var (current, error) = await RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!current)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
                        return;
                    }
                }

                await reload();
            };
            menu.Items.Add(cancel);
            return menu;
        }

        foreach (var action in IntersectedStorageActions(releaseIds, rowsById))
        {
            var act = action;
            var item = new MenuFlyoutItem { Text = StorageActionLabel(act) };
            item.Click += async (_, _) =>
            {
                var error = await RunStorageActionForReleases(act, releaseIds);
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                }
                else
                {
                    await reload();
                }
            };
            menu.Items.Add(item);
        }

        return menu;
    }

    // Of the given releases, those with uploads queued or in flight. Core omits
    // idle releases from the per-release map, so presence there is the signal.
    private async System.Threading.Tasks.Task<List<string>> UploadingReleases(
        List<string> releaseIds,
        TextBlock storageStatus)
    {
        var (current, result) = await RunForCurrentHandle(NativeBae.OutboxSnapshot);
        if (!current)
        {
            return new List<string>();
        }
        if (result.Error is not null)
        {
            storageStatus.Text = result.Error;
            storageStatus.Visibility = Visibility.Visible;
            return new List<string>();
        }
        var snapshot = result.Snapshot;
        if (snapshot is null)
        {
            // Couldn't read the outbox; surface it like the panel load does
            // rather than silently dropping the cancel action.
            storageStatus.Text = Loc.Chrome("outbox.read_failed");
            storageStatus.Visibility = Visibility.Visible;
            return new List<string>();
        }

        return releaseIds.Where(snapshot.PerRelease.ContainsKey).ToList();
    }

    // Of the given releases, those queued or downloading in the pin queue.
    private async System.Threading.Tasks.Task<List<string>> DownloadingReleases(
        List<string> releaseIds)
    {
        var (current, result) = await RunForCurrentHandle(NativeBae.DownloadSnapshot);
        if (!current)
        {
            return new List<string>();
        }
        if (result.Error is not null)
        {
            BaeDiagnostics.Logger.Warning(
                $"couldn't read the download snapshot; pin-cancel unavailable: {result.Error}");
            return new List<string>();
        }
        var snapshot = result.Snapshot;
        if (snapshot is null)
        {
            // The pin queue is in-memory and read is infallible bar a dropped
            // handle, so this is an app-state fault, not a user-facing read
            // error — log it and offer no pin-cancel rather than a toast.
            BaeDiagnostics.Logger.Warning(
                "couldn't read the download snapshot; pin-cancel unavailable");
            return new List<string>();
        }

        var pinning = snapshot.Downloads.Select(op => op.ReleaseId).ToHashSet();
        return releaseIds.Where(pinning.Contains).ToList();
    }

    // Run a storage transition on every release in the selection off the UI
    // thread. "unmanage" asks once for a destination folder, then moves each
    // release into it. Returns null on success (or a cancelled picker), else the
    // first error message.
    private async System.Threading.Tasks.Task<string?> RunStorageActionForReleases(
        BridgeReleaseStorageAction action, List<string> releaseIds)
    {
        if (action == BridgeReleaseStorageAction.MakeLocal)
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(
                picker, WinRT.Interop.WindowNative.GetWindowHandle(this));
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                return null;
            }

            var path = folder.Path;
            // Mark these releases as unmanaging so a right-click can cancel them
            // while the blocking transfer runs; cleared when it returns.
            foreach (var releaseId in releaseIds)
            {
                _unmanagingReleases.Add(releaseId);
            }
            try
            {
                var (storageActionCurrent, error) = await RunForCurrentHandle(handle =>
                {
                    foreach (var releaseId in releaseIds)
                    {
                        var error = NativeBae.MakeReleaseLocal(handle, releaseId, path);
                        if (error is not null)
                        {
                            return error;
                        }
                    }

                    return (string?)null;
                });
                return storageActionCurrent ? error : null;
            }
            finally
            {
                foreach (var releaseId in releaseIds)
                {
                    _unmanagingReleases.Remove(releaseId);
                }
            }
        }

        var (current, actionError) = await RunForCurrentHandle(handle =>
        {
            foreach (var releaseId in releaseIds)
            {
                var error = action switch
                {
                    BridgeReleaseStorageAction.Pin => NativeBae.PinRelease(handle, releaseId),
                    BridgeReleaseStorageAction.Unpin => NativeBae.UnpinRelease(handle, releaseId),
                    BridgeReleaseStorageAction.MakeRemote => NativeBae.MakeReleaseRemote(handle, releaseId, pin: false),
                    BridgeReleaseStorageAction.MakeLocal => throw new InvalidOperationException(
                        "make-local storage actions must choose a destination before running"),
                    _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
                };
                if (error is not null)
                {
                    return error;
                }
            }

            return (string?)null;
        });
        return current ? actionError : null;
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
