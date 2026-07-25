using System;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.System;
// `Windows.System` (needed for VirtualKey) also declares a DispatcherQueue;
// alias the WinUI one so the unqualified name stays unambiguous (CS0104).
using DispatcherQueue = Microsoft.UI.Dispatching.DispatcherQueue;

namespace Bae.Windows;

// The library content column: the album grid, the composer/artist lists, and the
// search overlay, with the mode heading/sort controls and the scroll-driven
// header collapse that frame all three. It owns the browse-mode state and the
// inline album expansion; the window hands it the ContentColumn's named elements
// plus the stores it drives and the status/shuffle-enabled surfaces it writes.
//
// The composer/artist detail columns and the search-result rows are rendered by
// BrowserPanes, which this view constructs and drives; RevealAlbum is the shared
// album entry point the now-playing jump and the import "view in library" banner
// call through.
internal sealed partial class LibraryBrowser
{
    private readonly SessionStore _session;
    private readonly LibraryService _library;
    private readonly MediaPathsService _mediaPaths;
    private readonly PlaybackService _playbackService;
    private readonly QueueService _queueService;
    private readonly DownloadsService _downloads;
    private readonly LibraryBrowserStore _browser;
    // The album grid's multi-selection: modifier clicks / Esc / Ctrl+A mutate it,
    // and the tint (bound OneWay) syncs from it after every mutation. A browse
    // concern owned wholly here.
    private readonly AlbumGridSelectionModel _albumSelection = new();
    private readonly Action<string, string> _showError;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly StorageStore _storage;
    private readonly TransferProgressStore _transferProgress;
    private readonly ProjectionRegistry _projections;
    private readonly QueuePane _queuePane;
    private readonly DispatcherQueue _dispatcher;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly Action<string> _setStatus;
    // The story-3 empty state: the current mode's empty-title chrome key, or
    // null when the mode has content (or the load failed — errors are status).
    private readonly Action<string?> _setEmptyState;
    private readonly Action<bool> _setShuffleLibraryEnabled;

    private readonly ListView _albumGrid;
    private readonly Grid _composerBrowser;
    private readonly Grid _artistBrowser;
    private readonly ListView _composerList;
    private readonly ListView _artistList;
    private readonly TextBlock _modeHeadingText;
    private readonly FontIcon _modeHeadingChevron;
    private readonly MenuFlyout _modeHeadingFlyout;
    private readonly Grid _headerBand;
    private readonly AutoSuggestBox _searchBox;
    private readonly Flyout _searchFlyout;

    private readonly LibrarySortControls _sortControls;
    private readonly HeaderCollapseModel _collapse = new();
    private readonly BrowserPanes _browserPanes;

    // Tracks whether the search dropdown is open, so a re-focus doesn't re-show it
    // while it already is (WinUI's FlyoutBase exposes no IsOpen).
    private bool _searchFlyoutOpen;

    public LibraryBrowser(
        SessionStore session,
        LibraryService library,
        MediaPathsService mediaPaths,
        PlaybackService playbackService,
        QueueService queueService,
        DownloadsService downloads,
        LibraryBrowserStore browser,
        Action<string, string> showError,
        ReleaseActionDialogs releaseActions,
        StorageStore storage,
        TransferProgressStore transferProgress,
        ProjectionRegistry projections,
        QueuePane queuePane,
        DispatcherQueue dispatcher,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        Action<string> setStatus,
        Action<string?> setEmptyState,
        Action<bool> setShuffleLibraryEnabled,
        ListView albumGrid,
        Grid composerBrowser,
        Grid artistBrowser,
        ListView composerList,
        ListView artistList,
        StackPanel composerDetailPane,
        StackPanel artistDetailPane,
        ListView searchResultsList,
        TextBlock modeHeadingText,
        FontIcon modeHeadingChevron,
        MenuFlyout modeHeadingFlyout,
        StackPanel sortControlsHost,
        Grid headerBand,
        AutoSuggestBox searchBox,
        Flyout searchFlyout,
        UIElement searchOverlayPassThrough)
    {
        _session = session;
        _library = library;
        _mediaPaths = mediaPaths;
        _playbackService = playbackService;
        _queueService = queueService;
        _downloads = downloads;
        _browser = browser;
        _showError = showError;
        _releaseActions = releaseActions;
        _storage = storage;
        _transferProgress = transferProgress;
        _projections = projections;
        _queuePane = queuePane;
        _dispatcher = dispatcher;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _setStatus = setStatus;
        _setEmptyState = setEmptyState;
        _setShuffleLibraryEnabled = setShuffleLibraryEnabled;
        _albumGrid = albumGrid;
        _composerBrowser = composerBrowser;
        _artistBrowser = artistBrowser;
        _composerList = composerList;
        _artistList = artistList;
        _modeHeadingText = modeHeadingText;
        _modeHeadingChevron = modeHeadingChevron;
        _modeHeadingFlyout = modeHeadingFlyout;
        _headerBand = headerBand;
        _searchBox = searchBox;
        _searchFlyout = searchFlyout;

        _sortControls = new LibrarySortControls(sortControlsHost, browser.Sort, ReloadBrowserForSortChange);
        BuildModeHeadingFlyout();
        SelectBrowserMode(BrowserMode.Albums, reload: false);

        // The composer/search panes: rendered and driven here through BrowserPanes,
        // which calls back for the pane visibility and album reveal this view owns.
        _browserPanes = new BrowserPanes(
            library,
            mediaPaths,
            dispatcher,
            searchResultsList,
            composerDetailPane,
            artistDetailPane,
            setStatus,
            ShowComposerBrowser,
            ShowArtistBrowser,
            RevealAlbum,
            _searchFlyout.Hide);

        // Escape in a non-empty search box clears it and restores the browse pane;
        // handledEventsToo so it fires even if the box marks Escape handled.
        _searchBox.AddHandler(UIElement.KeyDownEvent, new KeyEventHandler(OnSearchBoxKeyDown), handledEventsToo: true);
        // Re-open the results dropdown when focus returns to a non-empty field.
        _searchFlyout.Opened += (_, _) => _searchFlyoutOpen = true;
        _searchFlyout.Closed += (_, _) => _searchFlyoutOpen = false;
        _searchBox.GotFocus += OnSearchBoxGotFocus;
        _searchBox.QuerySubmitted += OnSearchSubmitted;
        // A click outside the open dropdown both dismisses it AND lands on its
        // target: routing the light-dismiss overlay's input to the window root lets
        // the click through instead of being swallowed.
        _searchFlyout.OverlayInputPassThroughElement = searchOverlayPassThrough;

        // The album tiles scroll under a header that collapses as any browse panel
        // scrolls; each panel's scroll drives the shared collapse model.
        AttachCollapseScroll(_albumGrid, "albums");
        AttachCollapseScroll(_composerList, "composers");
        AttachCollapseScroll(_artistList, "artists");

        // Fit the grid once it has a width, and on every later resize.
        _albumRows = new AlbumGridRows(browser.Albums);
        _albumGrid.Loaded += (_, _) => ApplyGridMetrics();
        // The grid ListView pages rows from the row projection over the flat album
        // collection; each realized row builds its cards (and, for the row holding
        // the expanded album, the inline detail panel) in OnAlbumRowChanging.
        _albumGrid.ItemsSource = _albumRows;
        _albumGrid.ContainerContentChanging += OnAlbumRowChanging;
        _albumGrid.SizeChanged += OnAlbumGridSizeChanged;

        _composerList.ItemsSource = browser.Composers;
        _composerList.ItemClick += OnComposerClick;
        _artistList.ItemsSource = browser.Artists;
        _artistList.ItemClick += OnArtistClick;
    }

    // Wire core's browse invalidations to this view's reload. The window's
    // composition root registers these against the projection registry it owns.
    public void ReloadBrowserFromInvalidation()
    {
        if (string.IsNullOrEmpty(_searchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    // The mode heading's flyout picks the browse mode. Each item switches the mode
    // and reloads; the heading itself carries the current mode's name.
    private void BuildModeHeadingFlyout()
    {
        foreach (var (mode, key) in new[]
        {
            (BrowserMode.Albums, "library.mode.albums"),
            (BrowserMode.Artists, "library.mode.artists"),
            (BrowserMode.Composers, "library.mode.composers"),
        })
        {
            var item = new MenuFlyoutItem { Text = Loc.Chrome(key) };
            var target = mode;
            item.Click += (_, _) => SelectBrowserMode(target, reload: true);
            _modeHeadingFlyout.Items.Add(item);
        }
    }

    // Switch the browse mode: update the sort pills for the mode's criteria, retitle
    // the heading, and (when a library is open) reload the grid.
    private void SelectBrowserMode(BrowserMode mode, bool reload)
    {
        _browser.Sort.SetMode(mode);
        _sortControls.Render();
        _modeHeadingText.Text = Loc.Chrome(mode switch
        {
            BrowserMode.Artists => "library.mode.artists",
            BrowserMode.Composers => "library.mode.composers",
            _ => "library.mode.albums",
        });
        if (reload)
        {
            ReloadBrowserForSortChange();
        }
    }

    // Fit the album grid to the current width. Public so the window can re-fit
    // after it changes the content column's width cap.
    public void ApplyGridMetrics()
    {
        if (_albumGrid.ActualWidth <= 0)
        {
            return;
        }
        var metrics = AlbumGridColumns.Compute(_albumGrid.ActualWidth);
        // The card content width is the cell minus its trailing gutter (each card
        // carries that gutter as a right margin so a row fills exactly). Update the
        // shared width first, then the column count — a width change that keeps the
        // count only re-sizes cards; a count change regroups the rows.
        _gridLayout.CardWidth = metrics.CellWidth - AlbumGridColumns.Gutter;
        _albumRows.ColumnCount = metrics.Columns;
    }

    private void OnAlbumGridSizeChanged(object sender, SizeChangedEventArgs e) => ApplyGridMetrics();

    // Drive the header collapse from a browse panel's scroll: progress tracks the
    // active panel's offset, and a settle snaps to the nearer end.
    private void AttachCollapseScroll(Control panel, string scroller)
    {
        panel.Loaded += (_, _) =>
        {
            if (FindScrollViewer(panel) is not { } viewer)
            {
                return;
            }
            viewer.ViewChanged += (s, args) =>
            {
                // Scrolling the content behind the search dropdown dismisses it,
                // matching a click-away.
                if (_searchFlyoutOpen)
                {
                    _searchFlyout.Hide();
                }
                var offset = ((ScrollViewer)s!).VerticalOffset;
                ApplyCollapse(_collapse.ReportScroll(scroller, offset));
                if (!args.IsIntermediate && _collapse.ReportSettled(scroller))
                {
                    ApplyCollapse(_collapse.Progress);
                }
            };
        };
    }

    // Apply a collapse fraction to the heading and band: the heading scrubs from 56
    // to 24 with its chevron and tracking, and the band's top/bottom padding tighten.
    private void ApplyCollapse(double progress)
    {
        _modeHeadingText.FontSize = 56 - 32 * progress;
        _modeHeadingText.CharacterSpacing = (int)Math.Round(-25 + 8 * progress);
        _modeHeadingChevron.FontSize = 16 - 7 * progress;
        _headerBand.Padding = new Thickness(22, 56 - 42 * progress, 22, 32 - 20 * progress);
    }

    // The first ScrollViewer inside a control's realized template — the internal
    // scroller a ListView/GridView hosts its items in.
    private static ScrollViewer? FindScrollViewer(DependencyObject root)
    {
        if (root is ScrollViewer viewer)
        {
            return viewer;
        }
        var count = VisualTreeHelper.GetChildrenCount(root);
        for (var i = 0; i < count; i++)
        {
            if (FindScrollViewer(VisualTreeHelper.GetChild(root, i)) is { } found)
            {
                return found;
            }
        }
        return null;
    }

    // Reload the active grid after a sort or mode change, but only when a library is
    // open and no search is active — search results keep their relevance order.
    private void ReloadBrowserForSortChange()
    {
        if (_session.CurrentHandleOrNull() != null && string.IsNullOrEmpty(_searchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    private void ShowAlbumBrowser()
    {
        _albumGrid.Visibility = Visibility.Visible;
        _composerBrowser.Visibility = Visibility.Collapsed;
        _artistBrowser.Visibility = Visibility.Collapsed;
    }

    private void ShowComposerBrowser()
    {
        _albumGrid.Visibility = Visibility.Collapsed;
        _composerBrowser.Visibility = Visibility.Visible;
        _artistBrowser.Visibility = Visibility.Collapsed;
    }

    private void ShowArtistBrowser()
    {
        _albumGrid.Visibility = Visibility.Collapsed;
        _composerBrowser.Visibility = Visibility.Collapsed;
        _artistBrowser.Visibility = Visibility.Visible;
    }

    // Load the active mode's grid through the store, then render the status line
    // from what it returns. The composer pane flips into view and clears its detail
    // column up front (matching the original order) before the load can bail; the
    // album pane flips only once a load lands, so a handle-gone bail leaves it as-is.
    public void LoadCurrentBrowserMode()
    {
        // Any grid reload or mode switch tears down an open inline expansion: its
        // album's row is about to be rebuilt (or the mode is leaving the grid), and
        // the panel holds live storage-band registrations to dispose. A reveal that
        // reloads first then re-expands runs this before it expands, so it is safe.
        CollapseAlbumExpansion();

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
        // Re-fit after the new items realize the wrap panel (a fresh ItemsSource
        // rebuilds it), so cell sizing follows the current width.
        _dispatcher.TryEnqueue(ApplyGridMetrics);
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
            _setShuffleLibraryEnabled(
                load.Result == BrowserLoadResult.Loaded && !load.IsEmpty);
        }

        switch (load.Result)
        {
            case BrowserLoadResult.HandleGone:
                return;
            case BrowserLoadResult.Failed:
                _setEmptyState(null);
                _setStatus(load.Error ?? Loc.Chrome("library.load_failed"));
                return;
            default:
                _setStatus(string.Empty);
                _setEmptyState(load.IsEmpty
                    ? load.Mode switch
                    {
                        BrowserMode.Composers => "library.no_composers",
                        BrowserMode.Artists => "library.no_artists",
                        _ => "library.empty",
                    }
                    : null);
                return;
        }
    }

    // Focus the search box (the window's Ctrl+F accelerator routes here).
    public void FocusSearch() => _searchBox.Focus(FocusState.Programmatic);

    // Reset every piece of per-library browse state for a library switch or close,
    // so nothing from the old library bleeds into the next one or the welcome
    // chooser: drop the inline expansion, the browse collections, the selection,
    // and the search, and clear the status and shuffle-enabled surfaces.
    public void Reset()
    {
        CollapseAlbumExpansion();
        _browser.Reset();
        _albumSelection.Clear();
        _searchBox.Text = string.Empty;
        _setStatus(string.Empty);
        _setEmptyState(null);
        _setShuffleLibraryEnabled(false);
    }
}
