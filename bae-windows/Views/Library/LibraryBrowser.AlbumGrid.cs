using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Windows.ApplicationModel.DataTransfer;
using Windows.System;

namespace Bae.Windows;

// LibraryBrowser: the album grid's row/card rendering and the inline detail
// expansion that replaces the old modal AlbumDetailDialog, plus the album-card
// click / right-tap / drag dispatch and the multi-selection tint. The grid is a
// ListView of row items (AlbumGridRow); each realized row is built here — its
// cards, and under the row that holds the expanded album, the AlbumExpansionPanel.
// This mirrors macOS's LazyVStack of rows, each followed by an expansion slot keyed
// off a single selected-album id.
internal sealed partial class LibraryBrowser
{
    // The page size a reveal pages the flat album collection forward by until the
    // target album is loaded.
    private const uint RevealPageSize = 200;

    // The card width every card binds to; updated on resize so a width change
    // that keeps the column count never rebuilds rows.
    private readonly AlbumGridLayout _gridLayout = new();

    // The row projection over the flat album collection, bound to the album grid.
    // Assigned in the constructor once the browser store exists.
    private AlbumGridRows _albumRows = null!;

    // The one album whose inline expansion is open, or null. Absolute set: a plain
    // card click sets it (toggling off the same id), the ✕ / a reveal clears or
    // moves it — mirroring macOS's UiStore.selectedAlbumId.
    private string? _selectedAlbumId;

    // The built expansion panel's root, placed under the host row while open; null
    // while its detail is still loading (the row shows a spinner) or when nothing
    // is expanded.
    private FrameworkElement? _expansionContent;

    // The live panel behind _expansionContent, holding the storage-band refresh
    // registrations; disposed on collapse.
    private AlbumExpansionPanel? _currentExpansion;

    // The system accent, read once for the card tint and expansion ring (the same
    // source FlashRow trusts). A mid-session accent change isn't repainted.
    private global::Windows.UI.Color? _accentColor;

    private global::Windows.UI.Color AccentColor =>
        _accentColor ??= new global::Windows.UI.ViewManagement.UISettings()
            .GetColorValue(global::Windows.UI.ViewManagement.UIColorType.Accent);

    // Build a realized row's visual: its cards, plus the expansion panel when this
    // row holds the selected album.
    private void OnAlbumRowChanging(ListViewBase sender, ContainerContentChangingEventArgs args)
    {
        if (args.InRecycleQueue)
        {
            return;
        }
        if (args.Item is not AlbumGridRow row)
        {
            return;
        }
        args.ItemContainer.Content = BuildRowVisual(row);
        args.Handled = true;
    }

    private FrameworkElement BuildRowVisual(AlbumGridRow row)
    {
        var cards = new StackPanel { Orientation = Orientation.Horizontal };
        foreach (var album in row.Cards)
        {
            cards.Children.Add(BuildAlbumCard(album));
        }

        var rowPanel = new StackPanel { HorizontalAlignment = HorizontalAlignment.Stretch };
        rowPanel.Children.Add(cards);
        if (_selectedAlbumId is not null && row.Cards.Any(album => album.Id == _selectedAlbumId))
        {
            if (_expansionContent is { } panel)
            {
                // The expansion panel is one element reused across the host row's
                // rebuilds and re-realizations; detach it from a prior (now
                // orphaned) row before re-adding, or WinUI rejects the second
                // parent.
                if (panel.Parent is Panel previous)
                {
                    previous.Children.Remove(panel);
                }
                rowPanel.Children.Add(panel);
            }
            else
            {
                // Its detail is still loading; the row shows a spinner until the
                // panel lands and the row is rebuilt.
                rowPanel.Children.Add(BuildExpansionLoading());
            }
        }
        return rowPanel;
    }

    private static FrameworkElement BuildExpansionLoading() => new ProgressRing
    {
        IsActive = true,
        Width = 32,
        Height = 32,
        Margin = new Thickness(0, 24, 0, 24),
        HorizontalAlignment = HorizontalAlignment.Center,
    };

    // One album card: the shared AlbumCardVisual tile (the same one the capture
    // scene and the component gallery build), with this window's live behavior
    // attached — the dynamic values re-bound OneWay so a resize / selection /
    // cover-load updates the card without a row rebuild, and the click, right-tap,
    // and drag wired per-card (the ListView virtualizes rows, not cards).
    private FrameworkElement BuildAlbumCard(Album album)
    {
        var parts = AlbumCardVisual.Build(
            album.Title,
            album.Artist,
            album.YearText,
            album.Cover,
            _gridLayout.CardWidth,
            album.SelectionTintOpacity,
            album.ExpansionRingOpacity,
            AccentColor);

        BindOneWay(parts.Card, FrameworkElement.WidthProperty, _gridLayout, nameof(AlbumGridLayout.CardWidth));
        BindOneWay(parts.Tint, UIElement.OpacityProperty, album, nameof(Album.SelectionTintOpacity));
        BindOneWay(parts.Ring, UIElement.OpacityProperty, album, nameof(Album.ExpansionRingOpacity));
        BindOneWay(parts.Cover, Image.SourceProperty, album, nameof(Album.Cover));

        parts.Card.DataContext = album;
        parts.Card.CanDrag = true;
        parts.Card.Tapped += OnAlbumCardTapped;
        parts.Card.RightTapped += OnAlbumCardRightTapped;
        parts.Card.DragStarting += OnAlbumCardDragStarting;
        return parts.Card;
    }

    private static void BindOneWay(FrameworkElement target, DependencyProperty property, object source, string path) =>
        target.SetBinding(property, new Binding
        {
            Source = source,
            Path = new PropertyPath(path),
            Mode = BindingMode.OneWay,
        });

    // Dispatch by the modifiers held at click time: Ctrl toggles the clicked
    // album, Shift extends the range from the anchor, and a plain click toggles
    // the album's inline detail expansion (clearing the multi-selection).
    // Modifier clicks never open the expansion. Per-card (the grid ListView
    // virtualizes rows, not cards), so the album is the card's DataContext.
    private void OnAlbumCardTapped(object sender, TappedRoutedEventArgs e)
    {
        if (_session.CurrentHandleOrNull() == null || (sender as FrameworkElement)?.DataContext is not Album album)
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

        ToggleAlbumExpansion(album);
    }

    // Right-click / long-press on an album card: the bulk-action menu for
    // whatever the card targets — the whole multi-selection (visible order)
    // for a member card, else just that card. Never mutates the selection.
    private void OnAlbumCardRightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (_session.CurrentHandleOrNull() == null || (sender as FrameworkElement)?.DataContext is not Album album)
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
        var element = (FrameworkElement)sender;
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
                _session.WithCurrentHandle(handle => NativeBae.PlayRelease(handle, releaseId, -1, false));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPlayNext: () =>
            {
                _session.WithCurrentHandle(handle => NativeBae.AddReleaseNext(handle, releaseId));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onAddToQueue: () =>
            {
                _session.WithCurrentHandle(handle => NativeBae.AddReleaseToQueue(handle, releaseId));
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
                _session.WithCurrentHandle(handle => NativeBae.PlayReleases(handle, primaryReleaseIds));
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
        var albumsById = _browser.Albums.ToDictionary(album => album.Id);
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
        for (var i = 0; i < _browser.Albums.Count; i++)
        {
            if (_browser.Albums[i].Id == id)
            {
                return i;
            }
        }
        return null;
    }

    private string? AlbumIdAt(int index) =>
        index >= 0 && index < _browser.Albums.Count ? _browser.Albums[index].Id : null;

    // Sync every loaded album's tint from the selection model. Called after
    // every mutation; O(loaded count), which is at most the first page (500).
    private void SyncAlbumSelectionTint()
    {
        foreach (var album in _browser.Albums)
        {
            album.IsSelected = _albumSelection.Contains(album.Id);
        }
    }

    private static bool IsModifierDown(VirtualKey key) =>
        Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(key)
            .HasFlag(global::Windows.UI.Core.CoreVirtualKeyStates.Down);

    // Start a drag from an album card: carry the album ids as the newline-joined
    // payload the queue pane decodes — the whole multi-selection (visible order)
    // when the pressed card is part of it, else just that card. Cancelled when no
    // library is open. Never mutates the selection. Per-card (the grid ListView's
    // own drag would carry the row), so the album is the card's DataContext.
    private void OnAlbumCardDragStarting(UIElement sender, DragStartingEventArgs e)
    {
        if (_session.CurrentHandleOrNull() == null || (sender as FrameworkElement)?.DataContext is not Album album)
        {
            e.Cancel = true;
            return;
        }
        var ids = _albumSelection.OrderedTargets(album.Id, AlbumPosition);
        e.Data.SetText(QueueDragPayload.Encode(ids));
        e.Data.RequestedOperation = DataPackageOperation.Copy;
    }

    // Escape over the album grid clears a non-empty multi-selection. Routed here
    // from the window's global key handler; returns whether it consumed the key.
    public bool HandleEscapeSelection()
    {
        if (_albumGrid.Visibility != Visibility.Visible || _albumSelection.IsEmpty)
        {
            return false;
        }
        _albumSelection.Clear();
        SyncAlbumSelectionTint();
        return true;
    }

    // Ctrl+A over the album grid selects every loaded album. Routed here from the
    // window's global key handler (which owns the focused-text-input guard) for any
    // 'A' press; the Ctrl-held and grid-visible checks are this view's, so a bare
    // 'A' falls through untouched. Returns whether it consumed the key.
    public bool HandleSelectAll()
    {
        if (_albumGrid.Visibility != Visibility.Visible || !IsModifierDown(VirtualKey.Control))
        {
            return false;
        }
        _albumSelection.SelectAll(_browser.Albums.Select(album => album.Id).ToList());
        SyncAlbumSelectionTint();
        return true;
    }

    // A plain card click toggles the expansion for its album (macOS's plain-click
    // behavior); a second click on the same card collapses it.
    private void ToggleAlbumExpansion(Album album)
    {
        if (_selectedAlbumId == album.Id)
        {
            CollapseAlbumExpansion();
            return;
        }
        _ = ExpandAlbum(album, scrollToTrackId: null, initialReleaseId: null);
    }

    // Open the inline expansion for an album: mark it, clear any multi-selection,
    // show the host row's spinner, then fetch the detail and place the built panel.
    // A newer expand/collapse taken while the fetch was in flight supersedes this
    // one (the async panel is dropped), the way macOS's reveal cancels on a newer
    // request.
    private async System.Threading.Tasks.Task ExpandAlbum(Album album, string? scrollToTrackId, string? initialReleaseId)
    {
        CollapseAlbumExpansion();
        _selectedAlbumId = album.Id;
        album.IsExpanded = true;
        _albumSelection.Clear();
        SyncAlbumSelectionTint();
        RefreshRowContainerFor(album.Id);

        var panel = new AlbumExpansionPanel(
            _session,
            _mediaPaths,
            _dispatcher,
            _xamlRoot,
            _windowHandle,
            _setStatus,
            _releaseActions,
            _storage,
            _transferProgress,
            _projections,
            CollapseAlbumExpansion);
        var root = await panel.BuildAsync(album, scrollToTrackId, initialReleaseId);
        if (_selectedAlbumId != album.Id || root is null)
        {
            panel.Dispose();
            return;
        }
        _currentExpansion = panel;
        _expansionContent = root;
        RefreshRowContainerFor(album.Id);
    }

    // Close the inline expansion: drop the panel (disposing its registrations),
    // clear the ring, and rebuild the (former) host row without the panel.
    private void CollapseAlbumExpansion()
    {
        if (_selectedAlbumId is null)
        {
            return;
        }
        var id = _selectedAlbumId;
        _selectedAlbumId = null;
        _currentExpansion?.Dispose();
        _currentExpansion = null;
        _expansionContent = null;
        foreach (var album in _browser.Albums)
        {
            if (album.Id == id)
            {
                album.IsExpanded = false;
            }
        }
        RefreshRowContainerFor(id);
    }

    // Rebuild the realized container of the row holding an album, so a selection
    // change (open / close / move) shows or hides the expansion without waiting
    // for a recycle. A row that isn't realized needs nothing — it builds correctly
    // from _selectedAlbumId when it later realizes.
    private void RefreshRowContainerFor(string albumId)
    {
        var row = _albumRows.FirstOrDefault(candidate => candidate.Cards.Any(album => album.Id == albumId));
        if (row is not null && _albumGrid.ContainerFromItem(row) is ListViewItem container)
        {
            container.Content = BuildRowVisual(row);
        }
    }

    // Reveal an album from outside the grid (now-playing jump, import "view in
    // library", a composer/artist/search result): switch to the album grid, page
    // the album's row in, scroll to it, and expand it. scrollToTrackId flashes a
    // track row inside the panel; initialReleaseId picks a starting release. A
    // bridge gap for the album index (not present under the current sort) is
    // surfaced, not worked around with a client-side scan.
    public async System.Threading.Tasks.Task RevealAlbum(
        string albumId, string? scrollToTrackId = null, string? initialReleaseId = null)
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        // Bring the album grid to the front if a different browse mode or a search
        // is showing.
        if (_browser.Sort.Mode != BrowserMode.Albums)
        {
            SelectBrowserMode(BrowserMode.Albums, reload: true);
        }
        else if (!string.IsNullOrEmpty(_searchBox.Text))
        {
            _searchBox.Text = string.Empty;
            _searchFlyout.Hide();
            LoadCurrentBrowserMode();
        }

        var (current, result) = await _session.RunForCurrentHandle(
            handle => NativeBae.AlbumIndex(handle, _browser.Sort.Albums.Items, albumId));
        if (!current)
        {
            return;
        }
        if (result.Error is not null)
        {
            _setStatus(result.Error);
            return;
        }
        if (result.Index is not long index)
        {
            // Not present under the active sort: nothing to reveal.
            return;
        }

        // The album's page may never have been fetched; page forward until it is,
        // then re-fit the rows so the target's row exists to scroll to.
        while (_browser.Albums.Count <= index && _browser.Albums.HasMoreItems)
        {
            await _browser.Albums.LoadMoreItemsAsync(RevealPageSize);
        }
        if (_browser.Albums.Count <= index)
        {
            return;
        }
        ApplyGridMetrics();
        var columnCount = Math.Max(1, _albumRows.ColumnCount);
        var rowIndex = (int)(index / columnCount);
        if (rowIndex < _albumRows.Count)
        {
            _albumGrid.ScrollIntoView(_albumRows[rowIndex]);
        }
        var target = _browser.Albums.FirstOrDefault(album => album.Id == albumId);
        if (target is not null)
        {
            await ExpandAlbum(target, scrollToTrackId, initialReleaseId);
        }
    }
}
