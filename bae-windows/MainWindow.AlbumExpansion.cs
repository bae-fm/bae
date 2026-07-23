using System;
using System.Collections.Generic;
using System.Linq;
using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// MainWindow: the album grid's row/card rendering and the inline detail
// expansion that replaces the old modal AlbumDetailDialog. The grid is a ListView
// of row items (AlbumGridRow); each realized row is built here — its cards, and
// under the row that holds the expanded album, the AlbumExpansionPanel. This
// mirrors macOS's LazyVStack of rows, each followed by an expansion slot keyed
// off a single selected-album id.
public sealed partial class MainWindow : Window
{
    // The page size a reveal pages the flat album collection forward by until the
    // target album is loaded.
    private const uint RevealPageSize = 200;

    // The card width every card binds to; updated on resize so a width change
    // that keeps the column count never rebuilds rows.
    private readonly AlbumGridLayout _gridLayout = new();

    // The row projection over the flat album collection, bound to AlbumGrid.
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

    // One album card: the art (with a hover-free selection tint and, when this is
    // the expanded album, an accent ring), title, artist, and year — the tile the
    // XAML DataTemplate used to render, now built in code so it can be laid out in
    // a row alongside the injected expansion. Width binds to the shared card width
    // and carries a trailing gutter so a row fills exactly; the click, right-tap,
    // and drag are per-card (the ListView virtualizes rows, not cards).
    private FrameworkElement BuildAlbumCard(Album album)
    {
        var tint = new Border
        {
            CornerRadius = new CornerRadius(10),
            Background = new SolidColorBrush(AccentColor) { Opacity = 0.22 },
        };
        BindOneWay(tint, UIElement.OpacityProperty, album, nameof(Album.SelectionTintOpacity));

        var art = new Border
        {
            CornerRadius = new CornerRadius(12),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Translation = new Vector3(0, 0, 16),
            Shadow = new ThemeShadow(),
        };
        art.SizeChanged += OnTileArtSizeChanged;
        var image = new Image { Stretch = Stretch.UniformToFill };
        BindOneWay(image, Image.SourceProperty, album, nameof(Album.Cover));
        art.Child = image;

        var ring = new Border
        {
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(2),
            BorderBrush = new SolidColorBrush(AccentColor),
            IsHitTestVisible = false,
        };
        BindOneWay(ring, UIElement.OpacityProperty, album, nameof(Album.ExpansionRingOpacity));

        var artHost = new Grid { HorizontalAlignment = HorizontalAlignment.Stretch };
        artHost.Children.Add(art);
        artHost.Children.Add(ring);

        var title = new TextBlock
        {
            Text = album.Title,
            FontSize = 15,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 8, 0, 0),
        };
        var artist = new TextBlock
        {
            Text = album.Artist,
            FontSize = 13,
            FontWeight = Microsoft.UI.Text.FontWeights.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        };
        // The year line keeps its height even when absent so tiles stay uniform.
        var year = new TextBlock
        {
            Text = album.YearText,
            FontSize = 12,
            FontWeight = Microsoft.UI.Text.FontWeights.Medium,
            Height = 16,
            Foreground = (Brush)Application.Current.Resources["TextFillColorTertiaryBrush"],
        };

        var content = new StackPanel { Spacing = 2, Padding = new Thickness(6) };
        content.Children.Add(artHost);
        content.Children.Add(title);
        content.Children.Add(artist);
        content.Children.Add(year);

        var card = new Grid
        {
            // The trailing gutter and bottom row gap read as spaced tiles, as
            // ItemsWrapGrid's equal cells did.
            Margin = new Thickness(0, 0, AlbumGridColumns.Gutter, AlbumGridColumns.RowGap),
            DataContext = album,
            CanDrag = true,
        };
        BindOneWay(card, FrameworkElement.WidthProperty, _gridLayout, nameof(AlbumGridLayout.CardWidth));
        card.Children.Add(tint);
        card.Children.Add(content);
        card.Tapped += OnAlbumCardTapped;
        card.RightTapped += OnAlbumCardRightTapped;
        card.DragStarting += OnAlbumCardDragStarting;
        return card;
    }

    private static void BindOneWay(FrameworkElement target, DependencyProperty property, object source, string path) =>
        target.SetBinding(property, new Binding
        {
            Source = source,
            Path = new PropertyPath(path),
            Mode = BindingMode.OneWay,
        });

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
            () => Content.XamlRoot,
            () => WinRT.Interop.WindowNative.GetWindowHandle(this),
            text => StatusText.Text = text,
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
        foreach (var album in Albums)
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
        if (row is not null && AlbumGrid.ContainerFromItem(row) is ListViewItem container)
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
    private async System.Threading.Tasks.Task RevealAlbum(
        string albumId, string? scrollToTrackId = null, string? initialReleaseId = null)
    {
        if (CurrentHandleOrNull() == null)
        {
            return;
        }

        // Bring the album grid to the front if a different browse mode or a search
        // is showing.
        if (_browser.Sort.Mode != BrowserMode.Albums)
        {
            SelectBrowserMode(BrowserMode.Albums, reload: true);
        }
        else if (!string.IsNullOrEmpty(SearchBox.Text))
        {
            SearchBox.Text = string.Empty;
            SearchFlyout.Hide();
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
            StatusText.Text = result.Error;
            return;
        }
        if (result.Index is not long index)
        {
            // Not present under the active sort: nothing to reveal.
            return;
        }

        // The album's page may never have been fetched; page forward until it is,
        // then re-fit the rows so the target's row exists to scroll to.
        while (Albums.Count <= index && _browser.Albums.HasMoreItems)
        {
            await _browser.Albums.LoadMoreItemsAsync(RevealPageSize);
        }
        if (Albums.Count <= index)
        {
            return;
        }
        ApplyGridMetrics();
        var columnCount = Math.Max(1, _albumRows.ColumnCount);
        var rowIndex = (int)(index / columnCount);
        if (rowIndex < _albumRows.Count)
        {
            AlbumGrid.ScrollIntoView(_albumRows[rowIndex]);
        }
        var target = Albums.FirstOrDefault(album => album.Id == albumId);
        if (target is not null)
        {
            await ExpandAlbum(target, scrollToTrackId, initialReleaseId);
        }
    }
}
