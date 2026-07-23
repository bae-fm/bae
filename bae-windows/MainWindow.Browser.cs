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

// MainWindow: the album/composer/artist browser and grid — projection
// registration, mode switching, grid metrics, header collapse, and grid status.
// Split out of MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
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

    private void ReloadBrowserFromInvalidation()
    {
        if (string.IsNullOrEmpty(SearchBox.Text))
        {
            LoadCurrentBrowserMode();
        }
    }

    // Escape clears a non-empty search box and drops back to the active browse
    // pane, standing in for a clear button (the AutoSuggestBox's query-icon slot
    // has no room for one).
    private void OnSearchBoxKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key == VirtualKey.Escape && !string.IsNullOrEmpty(SearchBox.Text))
        {
            SearchBox.Text = string.Empty;
            SearchFlyout.Hide();
            LoadCurrentBrowserMode();
            e.Handled = true;
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
            ModeHeadingFlyout.Items.Add(item);
        }
    }

    // Switch the browse mode: update the sort pills for the mode's criteria, retitle
    // the heading, and (when a library is open) reload the grid.
    private void SelectBrowserMode(BrowserMode mode, bool reload)
    {
        _browser.Sort.SetMode(mode);
        _sortControls.Render();
        ModeHeadingText.Text = Loc.Chrome(mode switch
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

    // Cap or free the content column per the synced full-width preference, then
    // re-fit the album grid to the new width.
    private void ApplyLibraryWidth()
    {
        ContentColumn.MaxWidth = _settings.Current?.LibraryFullWidth == true ? double.PositiveInfinity : 1240;
        ApplyGridMetrics();
    }

    // Fit the album grid's columns to the current width: equal cells at ~200 wide
    // with a 30 gutter, each card sized so a row fills exactly. Runs on size and
    // full-width changes; a no-op until the grid has a width.
    private void ApplyGridMetrics()
    {
        if (AlbumGrid.ActualWidth <= 0)
        {
            return;
        }
        var metrics = AlbumGridColumns.Compute(AlbumGrid.ActualWidth);
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
                    SearchFlyout.Hide();
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
        ModeHeadingText.FontSize = 56 - 32 * progress;
        ModeHeadingText.CharacterSpacing = (int)Math.Round(-25 + 8 * progress);
        ModeHeadingChevron.FontSize = 16 - 7 * progress;
        HeaderBand.Padding = new Thickness(22, 56 - 42 * progress, 22, 32 - 20 * progress);
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
    }

    private void ShowComposerBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Visible;
        ArtistBrowser.Visibility = Visibility.Collapsed;
    }

    private void ShowArtistBrowser()
    {
        AlbumGrid.Visibility = Visibility.Collapsed;
        ComposerBrowser.Visibility = Visibility.Collapsed;
        ArtistBrowser.Visibility = Visibility.Visible;
    }

    // Load the active mode's grid through the store, then render the status line
    // from what it returns. The composer pane flips into view and clears its detail
    // column up front (matching the original order) before the load can bail; the
    // album pane flips only once a load lands, so a handle-gone bail leaves it as-is.
    private void LoadCurrentBrowserMode()
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
        DispatcherQueue.TryEnqueue(ApplyGridMetrics);
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
}
