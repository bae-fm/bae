using System;
using System.Globalization;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// The library shell: the top bar (Library/Import switcher, search, gear), the
/// browse content column, the docked now-playing bar, the queue pane, and the
/// import section. Hosted by <see cref="MainWindow"/> as its content once a
/// library is open; the window keeps only the window-level concerns (bounds,
/// presenter min-size, close teardown, foreground).
///
/// This is the composition root for the open library's views: it reads the
/// domain services and stores off the <see cref="AppService"/> and builds the
/// element-driven controllers (the now-playing bar, the queue pane, the library
/// browser, the dialogs) around them and the XAML's named elements.
/// </summary>
public sealed partial class MainView : UserControl
{
    // The library session (handle + event subscription). MainView drives the
    // lifecycle transitions (open/switch/finish-open, teardown) around it; the
    // AppService is the bundle built around the same session.
    private readonly SessionStore _session;
    // Root object for the open library: the narrow domain services and the store
    // bundle, built once around the session. Views read their service/store off
    // this instead of the session + NativeBae directly.
    private readonly AppService _appService;
    // Returns to a fresh welcome window: the coordinator (App) shuts this library
    // down and swaps windows. Driven by the close-library button and the settings
    // remove flow.
    private readonly Func<System.Threading.Tasks.Task> _closeToWelcome;
    // The hosting window's HWND, for the dialogs that parent native pickers and
    // for the transport-controls access. MainView is a UserControl and has no
    // window handle of its own, so the host supplies it.
    private readonly Func<IntPtr> _windowHandle;

    // The library content column: album grid, composer/artist lists, and search,
    // with the mode/sort/collapse chrome around them. Owns its own browse state.
    private readonly LibraryBrowser _libraryBrowser;
    private readonly LibrariesDialog _librariesDialog;
    private readonly ApproveDeviceDialog _approveDialog;
    // The unlock prompt for switching to a locked library (the bootstrap unlock
    // lives on the welcome window). Its re-open runs OpenLibrary in this view.
    private readonly UnlockDialog _unlockDialog;
    private readonly NowPlayingBarController _nowPlayingBar;
    private readonly ImportSection _importSection;
    private readonly ImportPickerDialog _importPicker;
    private readonly ImportConfirmDialog _importConfirm;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly QueuePane _queuePane;
    private readonly LightboxOverlay _lightbox;
    private readonly StorageDialog _storageDialog;
    private readonly MembersPane _membersPane;
    private readonly SettingsWindow _settingsWindow;

    // Drives the settings "updates" section: a launch-time background check and
    // the manual check/download/apply from settings. Inert on a dev run or a
    // loose-zip copy (IsAvailable is false).
    private readonly UpdateService _updateService = new();

    // Built after the coordinator opens the handle and the host constructs the
    // AppService around it. Reads its stores/services off the AppService, builds
    // the element-driven controllers, wires the store↔view handlers, and finishes
    // the open (FinishOpen). closeToWelcome returns to the welcome window via the
    // coordinator; windowHandle is the host window's HWND.
    internal MainView(
        AppService appService,
        SessionStore session,
        Func<System.Threading.Tasks.Task> closeToWelcome,
        Func<IntPtr> windowHandle)
    {
        _session = session;
        _appService = appService;
        _closeToWelcome = closeToWelcome;
        _windowHandle = windowHandle;

        InitializeComponent();

        // Bind layout direction to the UI locale: ar/he (and any other RTL
        // culture) lay out right-to-left. The whole tree inherits from the root
        // grid, so this is the single place the app decides direction. macOS gets
        // this from the system; on Windows the app sets it from the culture.
        RootGrid.FlowDirection = CultureInfo.CurrentUICulture.TextInfo.IsRightToLeft
            ? FlowDirection.RightToLeft
            : FlowDirection.LeftToRight;

        // The top bar per desktop story 3: the Library/Import switcher pill,
        // the search field, and the settings gear. The gear opens Settings on
        // click, like macOS; its right-click flyout carries the commands macOS
        // keeps in the menu bar (chrome Windows doesn't have).
        LibrarySegment.Content = Loc.Chrome("section.library");
        ImportSegment.Content = Loc.Chrome("section.import");
        StyleSectionPill();
        SetIconButtonLabel(SettingsButton, "toolbar.settings");
        LibrariesMenuItem.Text = Loc.Chrome("toolbar.libraries");
        StorageMenuItem.Text = Loc.Chrome("toolbar.storage");
        CloseLibraryMenuItem.Text = Loc.Chrome("toolbar.close_library");

        _appService.ShellStore.Changed += RenderBanner;
        _appService.SyncStatusStore.Changed += RenderSyncStatus;
        // The content column's width cap follows the synced full-width preference.
        _appService.SettingsStore.Changed += ApplyLibraryWidth;

        _nowPlayingBar = new NowPlayingBarController(
            _session,
            _appService.MediaPaths,
            _appService.Playback,
            _appService.Queue,
            _appService.PlaybackStore,
            _appService.CastStore,
            // Read at call time: the settings mirror is seeded from the handle
            // once a library is open.
            () => _appService.SettingsStore.Current?.ShowRemainingTime ?? false,
            () => XamlRoot,
            NowPlayingBar,
            NpCover,
            NpTitle,
            NpArtist,
            NpElapsed,
            NpDuration,
            NpProgress,
            NpVolume,
            NpPlayPause,
            NpPlayGlyph,
            NpMute,
            NpMuteGlyph,
            NpRepeat,
            NpRepeatGlyph,
            NpShuffle,
            NpShuffleGlyph,
            NpPrev,
            NpNext,
            NpLoading,
            NpCast,
            NpCastGlyph,
            QueueAddBadge,
            QueueAddBadgeScale,
            QueueAddBadgeText,
            NpPlayScale,
            NpCoverFrame,
            // Read at call time: the library browser is constructed further down.
            (albumId, trackId) => _libraryBrowser.RevealAlbum(albumId, trackId));
        // The bar's art and title open the playing album; the tooltip is the
        // affordance that they're clickable.
        ToolTipService.SetToolTip(NpCoverFrame, Loc.Chrome("nowplaying.go_to_album"));
        ToolTipService.SetToolTip(NpTitle, Loc.Chrome("nowplaying.go_to_album"));
        // The icon-only transport buttons whose meaning never changes get a fixed
        // accessible name and tooltip (the glyph alone exposes nothing to
        // Narrator). The state-dependent ones — play/pause, mute, repeat, shuffle
        // — are named by the controller as their state renders.
        SetIconButtonLabel(NpPrev, "nowplaying.previous");
        SetIconButtonLabel(NpNext, "nowplaying.next");
        SetIconButtonLabel(NpQueue, "queue.title");

        _session.UiEvent += _appService.UiEventRouter.Route;

        // The artwork lightbox: opened from the album gallery and the import
        // confirmation's local artwork tiles.
        _lightbox = new LightboxOverlay(() => XamlRoot);

        // The per-release action dialogs the inline album expansion opens. The
        // expansion panel itself is built per open in ExpandAlbum from these
        // stores; it is the shared album entry point from the grid, the panes, the
        // now-playing jump, and the import "view in library" banner.
        _releaseActions = new ReleaseActionDialogs(
            _session,
            _appService.MediaPaths,
            () => XamlRoot,
            _windowHandle,
            text => StatusText.Text = text,
            _appService.ProjectionRegistry,
            _lightbox);
        _queuePane = new QueuePane(
            _session,
            _appService.MediaPaths,
            _appService.Library,
            _appService.Queue,
            _appService.PlaybackStore,
            QueuePaneHost,
            message => _appService.ShowError(Loc.Chrome("error.playback_title"), message));
        // The now-playing bar's queue button toggles the pane and is an append drop
        // target; the pane owns that wiring.
        _queuePane.AttachToggleButton(NpQueue);

        // The storage sheet. The dialog registers its live-refresh handlers on the
        // projection registry while open.
        _storageDialog = new StorageDialog(
            _session,
            () => XamlRoot,
            _windowHandle,
            _appService.StorageStore,
            _appService.Downloads,
            _appService.ProjectionRegistry,
            _appService.TransferProgressStore);

        // The library content column: the album grid, composer/artist lists, and
        // search, with the mode/sort/collapse chrome. The window stays the
        // navigation shell (open/close/switch) and hands the browser the
        // ContentColumn's named elements plus the status/shuffle surfaces it writes.
        _libraryBrowser = new LibraryBrowser(
            _session,
            _appService.Library,
            _appService.MediaPaths,
            _appService.Playback,
            _appService.Queue,
            _appService.Downloads,
            _appService.LibraryBrowserStore,
            _appService.ShowError,
            _releaseActions,
            _appService.StorageStore,
            _appService.TransferProgressStore,
            _appService.ProjectionRegistry,
            _queuePane,
            DispatcherQueue,
            () => XamlRoot,
            _windowHandle,
            text => StatusText.Text = text,
            RenderEmptyState,
            enabled => ShuffleLibraryItem.IsEnabled = enabled,
            AlbumGrid,
            ComposerBrowser,
            ArtistBrowser,
            ComposerList,
            ArtistList,
            ComposerDetailPane,
            ArtistDetailPane,
            SearchResultsList,
            ModeHeadingText,
            ModeHeadingChevron,
            ModeHeadingFlyout,
            SortControls,
            HeaderBand,
            SearchBox,
            SearchFlyout,
            RootGrid);
        _unlockDialog = new UnlockDialog(() => XamlRoot, text => StatusText.Text = text, OpenLibrary);
        _approveDialog = new ApproveDeviceDialog(
            () => XamlRoot,
            _windowHandle,
            _session);
        _librariesDialog = new LibrariesDialog(
            () => XamlRoot,
            _session,
            LoadLibraries,
            CreateLibraryOrReport,
            SwitchLibrary);

        // The import flow: the confirm step (which can open an album), the picker
        // that leads to it, and the folder-scan dialog that opens the picker.
        _importConfirm = new ImportConfirmDialog(
            _session, () => XamlRoot, albumId => _libraryBrowser.RevealAlbum(albumId), _lightbox);
        _importPicker = new ImportPickerDialog(
            _session, () => XamlRoot, _appService.ImportStore, _appService.Playback, _importConfirm);
        _importSection = new ImportSection(
            _session,
            _windowHandle,
            _appService.ImportStore,
            _importPicker,
            ShowImportBanner,
            () => SwitchSection(import: true));

        // The settings window and its store/panes. It registers its config
        // re-read on the projection registry while open, opens the approve flow
        // for add-device, and shares the one UpdateService with the launch-time
        // check.
        _membersPane = new MembersPane(_session);
        _settingsWindow = new SettingsWindow(
            _session,
            DispatcherQueue,
            _appService.SettingsStore,
            _membersPane,
            _approveDialog,
            _updateService,
            _appService.ProjectionRegistry,
            OpenLibrary,
            _closeToWelcome);

        RegisterProjections();

        // Load the initial browse content now: the album grid, or the
        // empty-library state when the library has nothing in it. The coordinator
        // opened the handle before this view was built, so this reads it directly.
        // The session-lifecycle bring-up (seed playback, subscribe to core events,
        // report the screen) is the host's follow-up FinishOpen call — kept
        // separate so a stubbed scene renders this content off a stub LibraryService
        // without ever touching a live handle.
        _libraryBrowser.LoadCurrentBrowserMode();

#if DEBUG
        // Debug-only: the component-gallery toolbar entry (the WinUI preview
        // analogue). Compiled out of Release.
        DebugToolbarButton.Attach(ToolbarButtons);
#endif

        // Check for an update in the background at launch, like macOS's Sparkle
        // check-on-appear. Fire-and-forget: the service catches and logs every
        // failure and this is async I/O, so it never blocks startup.
        _ = _updateService.CheckInBackgroundAsync();
    }

    // Wire core's invalidations to the reloads they drive. Static consumers (the
    // library browser, sync status, import candidates) register for the view's
    // lifetime; the storage and settings dialogs supply their live-refresh
    // callbacks while open. Composition-root wiring: the handlers live in the
    // views/stores, the registration is the view's.
    private void RegisterProjections()
    {
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.AlbumList), _libraryBrowser.ReloadBrowserFromInvalidation);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.ComposerList), _libraryBrowser.ReloadBrowserFromInvalidation);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.ArtistList), _libraryBrowser.ReloadBrowserFromInvalidation);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.SyncStatus), _appService.SyncStatusStore.Refresh);
        // Config changes reach the now-playing bar's time-label mode, which is a
        // synced preference: flipping it on another device re-renders the bar here.
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.Config), OnConfigInvalidated);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.ImportCandidateList), _appService.ImportStore.RefreshCandidates);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.ImportCandidate), _appService.ImportStore.RefreshCandidates);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.WatchedFolders), _appService.ImportStore.RefreshCandidates);
        _appService.ProjectionRegistry.Register(typeof(BridgeInvalidation.CastDevices), _appService.CastStore.RefreshDevices);
    }

    private void OnConfigInvalidated()
    {
        _appService.SettingsStore.Reload();
        _nowPlayingBar.RefreshTimeLabelMode();
    }

    // Cap or free the content column per the synced full-width preference, then
    // re-fit the album grid to the new width. The width cap is the shell skeleton's
    // (ContentColumn); the grid re-fit is the browser's.
    private void ApplyLibraryWidth()
    {
        ContentColumn.MaxWidth = _appService.SettingsStore.Current?.LibraryFullWidth == true ? double.PositiveInfinity : 1240;
        _libraryBrowser.ApplyGridMetrics();
    }
}
