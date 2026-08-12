using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Data;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The library content column (desktop story 3): the large mode heading — itself a
// dropdown switching albums / artists / composers — with the sort controls
// opposite, over the mode's pane. The album pane is the incremental-loading grid
// with inline expansion; the composer and artist panes are incremental lists. Core
// list subscriptions reload the active list in place. The empty state lives inside
// each pane, so an empty library shows the mode's placeholder here.
internal sealed class LibraryBrowserView : UserControl
{
    private readonly AppService _app;
    private readonly TextBlock _headingText;
    private readonly StackPanel _sortHost;
    private readonly LibrarySortControls _sortControls;

    private readonly AlbumGridView _albumPane;
    private readonly IncrementalListView<ComposerSummary> _composerPane;
    private readonly IncrementalListView<ArtistSummary> _artistPane;

    public LibraryBrowserView(AppService app, ReleaseActionDialogs dialogs)
    {
        _app = app;
        var store = app.LibraryBrowserStore;

        _albumPane = new AlbumGridView(app, dialogs);
        _composerPane = new IncrementalListView<ComposerSummary>(
            () => store.Composers,
            store.ComposerById,
            composer => BrowseRow.Build(composer, composer.Name, composer.WorkCountText),
            "library.no_composers");
        _artistPane = new IncrementalListView<ArtistSummary>(
            () => store.Artists,
            store.ArtistById,
            artist => BrowseRow.Build(artist, artist.Name, artist.AlbumCountText),
            "library.no_artists");

        var pane = new Panel();
        pane.Children.Add(_albumPane);
        pane.Children.Add(_composerPane);
        pane.Children.Add(_artistPane);

        // ── Header: mode heading (dropdown) + sort controls ─────────────────────
        _headingText = new TextBlock
        {
            FontSize = 56,
            FontWeight = FontWeight.ExtraBold,
            VerticalAlignment = VerticalAlignment.Bottom,
        };
        _headingText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var chevron = Icons.Glyph(Icons.ChevronDown, 18, "BaeTextSecondaryBrush");
        chevron.VerticalAlignment = VerticalAlignment.Bottom;
        chevron.Margin = new Thickness(0, 0, 0, 12);
        var heading = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Bottom,
        };
        heading.Children.Add(_headingText);
        heading.Children.Add(chevron);
        var headingButton = new Button
        {
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
            Content = heading,
            Flyout = BuildModeFlyout(),
        };

        _sortHost = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Bottom,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        _sortControls = new LibrarySortControls(_sortHost, store.Sort);

        var header = new Grid
        {
            Margin = new Thickness(22, 56, 22, 32),
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
        };
        Grid.SetColumn(headingButton, 0);
        Grid.SetColumn(_sortHost, 1);
        header.Children.Add(headingButton);
        header.Children.Add(_sortHost);

        var column = new Grid
        {
            MaxWidth = 1240,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            RowDefinitions = new RowDefinitions("Auto,*"),
        };
        Grid.SetRow(header, 0);
        Grid.SetRow(pane, 1);
        column.Children.Add(header);
        column.Children.Add(pane);
        Content = column;

        // A sort change swaps a list instance; rebind the affected pane.
        store.ListSwapped += mode =>
        {
            if (mode == BrowserMode.Composers)
            {
                _composerPane.Rebind();
            }
            else if (mode == BrowserMode.Artists)
            {
                _artistPane.Rebind();
            }
        };

        SelectMode(BrowserMode.Albums);
    }

    // Reveal an album in the grid (the import confirm's "view in library" jump, and
    // any future navigate-to-album): switch to the album mode, then page its row in
    // and expand it.
    public System.Threading.Tasks.Task RevealAlbum(string albumId)
    {
        SelectMode(BrowserMode.Albums);
        return _albumPane.RevealAlbum(albumId);
    }

    private MenuFlyout BuildModeFlyout()
    {
        var flyout = new MenuFlyout();
        foreach (var (mode, key) in new[]
        {
            (BrowserMode.Albums, "library.mode.albums"),
            (BrowserMode.Artists, "library.mode.artists"),
            (BrowserMode.Composers, "library.mode.composers"),
        })
        {
            var target = mode;
            var item = new MenuItem { Header = Loc.Chrome(key) };
            item.Click += (_, _) => SelectMode(target);
            flyout.Items.Add(item);
        }
        return flyout;
    }

    private void SelectMode(BrowserMode mode)
    {
        _app.LibraryBrowserStore.Sort.SetMode(mode);
        _app.LibraryBrowserStore.EnsureLoaded(mode);
        _headingText.Text = Loc.Chrome(mode switch
        {
            BrowserMode.Artists => "library.mode.artists",
            BrowserMode.Composers => "library.mode.composers",
            _ => "library.mode.albums",
        });
        _sortControls.Render();
        _albumPane.IsVisible = mode == BrowserMode.Albums;
        _composerPane.IsVisible = mode == BrowserMode.Composers;
        _artistPane.IsVisible = mode == BrowserMode.Artists;
    }
}

// One composer / artist browse row: a small cover thumbnail, the name, and a
// secondary count line. The click-through to a composer / artist detail pane is a
// later browser-panes concern; this renders the browsable row.
internal static class BrowseRow
{
    // coverSource exposes a `Cover` bitmap property that fills in as the async load
    // lands (ComposerSummary / ArtistSummary), so the thumbnail binds OneWay to it.
    public static Control Build(object coverSource, string name, string secondary)
    {
        var image = new Image { Stretch = Stretch.UniformToFill };
        image[!Image.SourceProperty] = new Binding("Cover") { Source = coverSource, Mode = BindingMode.OneWay };
        var thumb = new Border
        {
            Width = 44,
            Height = 44,
            CornerRadius = new CornerRadius(8),
            ClipToBounds = true,
            Child = image,
        };
        thumb[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");

        var nameText = new TextBlock
        {
            Text = name,
            FontSize = 15,
            FontWeight = FontWeight.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        nameText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var secondaryText = new TextBlock
        {
            Text = secondary,
            FontSize = 12,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        secondaryText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var text = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(nameText);
        text.Children.Add(secondaryText);

        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            Margin = new Thickness(22, 6),
        };
        row.Children.Add(thumb);
        row.Children.Add(text);
        return row;
    }
}
