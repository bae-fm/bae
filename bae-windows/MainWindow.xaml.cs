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
    // The library session (handle + event subscription) and the store the library
    // browser reads. The store is constructed here (composition root) and handed to
    // the browser view along with the ContentColumn's named elements.
    private readonly SessionStore _session;
    private readonly LibraryBrowserStore _browser;
    // The library content column: album grid, composer/artist lists, and search,
    // with the mode/sort/collapse chrome around them. Owns its own browse state.
    private readonly LibraryBrowser _libraryBrowser;
    private readonly WelcomeView _welcomeView;
    private readonly LibrariesDialog _librariesDialog;
    private readonly JoinLibraryDialog _joinDialog;
    private readonly ApproveDeviceDialog _approveDialog;
    private readonly UnlockDialog _unlockDialog;
    private readonly RestoreFromCloudDialog _restoreCloudDialog;
    private readonly ShellStore _shell;
    private readonly SyncStatusStore _sync;
    private readonly PlaybackStore _playback;
    private readonly CastStore _cast;
    private readonly ProjectionRegistry _projections;
    private readonly TransferProgressStore _transferProgress;
    private readonly UiEventRouter _router;
    private readonly NowPlayingBarController _nowPlayingBar;
    private readonly ImportStore _import;
    private readonly ImportDialog _importDialog;
    private readonly ImportPickerDialog _importPicker;
    private readonly ImportConfirmDialog _importConfirm;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly QueuePane _queuePane;
    private readonly LightboxOverlay _lightbox;
    private readonly StorageStore _storage;
    private readonly StorageDialog _storageDialog;
    private readonly SettingsStore _settings;
    private readonly MembersPane _membersPane;
    private readonly SettingsWindow _settingsWindow;

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

    public MainWindow()
    {
        // The session and the browse store the library browser reads. The store
        // sets its collections' ItemsSource in code (no x:Bind), so nothing here
        // needs to exist before InitializeComponent.
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

        // The toolbar buttons are icon-only; each carries its former text label as
        // the tooltip and accessible name (reusing the existing x:Uid strings).
        SetIconButtonLabel(PlaybackMenuButton, "toolbar.playback");
        SetIconButtonLabel(LibrariesButton, "toolbar.libraries");
        SetIconButtonLabel(ImportButton, "toolbar.import");
        SetIconButtonLabel(StorageButton, "toolbar.storage");
        SetIconButtonLabel(SettingsButton, "toolbar.settings");
        // Close keeps its own more descriptive, localized "close library" tooltip
        // (set by x:Uid); give it the glyph here so x:Uid's Content doesn't win, and
        // read that tooltip back as the accessible name.
        CloseLibraryButton.Content = new FontIcon { Glyph = "\uE8BB", FontSize = 16 };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            CloseLibraryButton, ToolTipService.GetToolTip(CloseLibraryButton) as string ?? string.Empty);

        _shell = new ShellStore();
        _shell.Changed += RenderBanner;
        _playback = new PlaybackStore();
        _cast = new CastStore(_session);
        _sync = new SyncStatusStore(_session);
        _sync.Changed += RenderSyncStatus;
        _nowPlayingBar = new NowPlayingBarController(
            _session,
            _playback,
            _cast,
            // Read at call time: the settings mirror is built further down, and
            // seeded from the handle once a library is open.
            () => _settings.Current?.ShowRemainingTime ?? false,
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
        _import = new ImportStore(_session, _shell, _mediaControls);
        _projections = new ProjectionRegistry();
        // In-flight release transfers (pin/unpin/manage/unmanage), driven by core's
        // ReleaseTransferProgress/Ended events the router routes, read by the
        // storage dialog rows and the album-detail storage band. Constructed before
        // the router so it can route into it, and shared with the stores below.
        _transferProgress = new TransferProgressStore();
        _router = new UiEventRouter(
            _playback, _shell, _projections, _mediaControls, _import.HandlePreviewEvent, _transferProgress, _cast);
        _session.UiEvent += _router.Route;

        // The artwork lightbox: opened from the album gallery and the import
        // confirmation's local artwork tiles.
        _lightbox = new LightboxOverlay(() => Content.XamlRoot);

        // The storage sheet's non-UI operations, shared by the storage dialog and
        // the album-detail storage band so both run transitions the same way.
        _storage = new StorageStore(_session, _transferProgress);

        // The per-release action dialogs the inline album expansion opens. The
        // expansion panel itself is built per open in ExpandAlbum from these
        // stores; it is the shared album entry point from the grid, the panes, the
        // now-playing jump, and the import "view in library" banner.
        _releaseActions = new ReleaseActionDialogs(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
            _projections,
            _lightbox);
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

        // The library content column: the album grid, composer/artist lists, and
        // search, with the mode/sort/collapse chrome. The window stays the
        // navigation shell (open/close/switch) and hands the browser the
        // ContentColumn's named elements plus the status/shuffle surfaces it writes.
        _libraryBrowser = new LibraryBrowser(
            _session,
            _browser,
            _shell,
            _releaseActions,
            _storage,
            _transferProgress,
            _projections,
            _queuePane,
            DispatcherQueue,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
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
            _session, () => Content.XamlRoot, albumId => _libraryBrowser.RevealAlbum(albumId), _lightbox);
        _importPicker = new ImportPickerDialog(_session, () => Content.XamlRoot, _import, _importConfirm);
        _importDialog = new ImportDialog(
            _session,
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            _import,
            _importPicker);

        // The settings window and its store/panes. It registers its config
        // re-read on the projection registry while open, opens the approve flow
        // for add-device, and shares the one UpdateService with the launch-time
        // check.
        _settings = new SettingsStore(_session);
        // The content column's width cap follows the synced full-width preference.
        _settings.Changed += ApplyLibraryWidth;
        _membersPane = new MembersPane(_session);
        _settingsWindow = new SettingsWindow(
            _session,
            DispatcherQueue,
            _settings,
            _membersPane,
            _approveDialog,
            _updateService,
            _projections,
            OpenLibrary,
            CloseLibrary);

        RegisterProjections();

        LoadLibrary();

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
    // library browser, sync status, import candidates) register for the window's
    // lifetime; the storage and settings dialogs supply their live-refresh
    // callbacks while open. Composition-root wiring: the handlers live in the
    // views/stores, the registration is the window's.
    private void RegisterProjections()
    {
        _projections.Register(typeof(BridgeInvalidation.AlbumList), _libraryBrowser.ReloadBrowserFromInvalidation);
        _projections.Register(typeof(BridgeInvalidation.ComposerList), _libraryBrowser.ReloadBrowserFromInvalidation);
        _projections.Register(typeof(BridgeInvalidation.ArtistList), _libraryBrowser.ReloadBrowserFromInvalidation);
        _projections.Register(typeof(BridgeInvalidation.SyncStatus), _sync.Refresh);
        // Config changes reach the now-playing bar's time-label mode, which is a
        // synced preference: flipping it on another device re-renders the bar here.
        _projections.Register(typeof(BridgeInvalidation.Config), OnConfigInvalidated);
        _projections.Register(typeof(BridgeInvalidation.ImportCandidateList), _import.RefreshCandidates);
        _projections.Register(typeof(BridgeInvalidation.ImportCandidate), _import.RefreshCandidates);
        _projections.Register(typeof(BridgeInvalidation.WatchedFolders), _import.RefreshCandidates);
        _projections.Register(typeof(BridgeInvalidation.CastDevices), _cast.RefreshDevices);
    }

    private void OnConfigInvalidated()
    {
        _settings.Reload();
        _nowPlayingBar.RefreshTimeLabelMode();
    }

    // Cap or free the content column per the synced full-width preference, then
    // re-fit the album grid to the new width. The width cap is the shell skeleton's
    // (ContentColumn); the grid re-fit is the browser's.
    private void ApplyLibraryWidth()
    {
        ContentColumn.MaxWidth = _settings.Current?.LibraryFullWidth == true ? double.PositiveInfinity : 1240;
        _libraryBrowser.ApplyGridMetrics();
    }

}
