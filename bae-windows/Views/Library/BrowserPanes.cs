using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using Microsoft.UI;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Renders the three library panes that aren't the album grid: the
// search-results list and the composer/work and artist detail columns. Both
// fetch through the library and media-paths services (composer/work/artist
// detail, covers); the window owns pane visibility and the status line, passed
// in as callbacks. Opening an album is injected because the album-detail
// dialog lives on the window.
internal sealed partial class BrowserPanes
{
    // One row of the search-results dropdown: a section header, a status message
    // (no matches / failed), or a clickable result. Rendered in one virtualized
    // ListView (sections as items, not a ScrollViewer over a StackPanel of
    // hand-built rows) — the bridge already caps results at 50/section (~200 rows
    // total), so this bounds the *render* cost, not the data; there is no paging.
    private abstract record SearchResultRow;

    private sealed record SearchHeaderRow(string Title) : SearchResultRow;

    private sealed record SearchMessageRow(string Text) : SearchResultRow;

    // A result row: every kind (album, track, composer, work) uses the same
    // layout with a 46x46 cover slot. The cover source exposes a "Cover"
    // ImageSource and raises PropertyChanged as it loads; when it has none, the
    // slot's card fill stands in.
    private sealed record ArtResultRow(
        object CoverSource, string Title, string Subtitle, string? Trailing, Func<System.Threading.Tasks.Task> Action)
        : SearchResultRow;

    private readonly LibraryService _library;
    private readonly ImageStore _images;
    private readonly DispatcherQueue _dispatcher;
    private readonly ListView _searchResultsList;
    private readonly ObservableCollection<SearchResultRow> _searchResults = new();
    private readonly StackPanel _composerDetailPane;
    private readonly StackPanel _artistDetailPane;
    private readonly Action<string> _setStatus;
    private readonly Action _showComposerBrowser;
    private readonly Action _showArtistBrowser;
    private readonly Func<string, string?, string?, System.Threading.Tasks.Task> _openAlbum;
    private readonly Action _dismissSearch;

    public BrowserPanes(
        LibraryService library,
        ImageStore images,
        DispatcherQueue dispatcher,
        ListView searchResultsList,
        StackPanel composerDetailPane,
        StackPanel artistDetailPane,
        Action<string> setStatus,
        Action showComposerBrowser,
        Action showArtistBrowser,
        Func<string, string?, string?, System.Threading.Tasks.Task> openAlbum,
        Action dismissSearch)
    {
        _library = library;
        _images = images;
        _dispatcher = dispatcher;
        _dismissSearch = dismissSearch;
        _searchResultsList = searchResultsList;
        _searchResultsList.ItemsSource = _searchResults;
        _searchResultsList.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue || args.Item is not SearchResultRow row)
            {
                return;
            }
            args.ItemContainer.Content = BuildSearchRowVisual(row);
            args.Handled = true;
        };
        _composerDetailPane = composerDetailPane;
        _artistDetailPane = artistDetailPane;
        _setStatus = setStatus;
        _showComposerBrowser = showComposerBrowser;
        _showArtistBrowser = showArtistBrowser;
        _openAlbum = openAlbum;
    }

    public void ClearComposerDetail() => _composerDetailPane.Children.Clear();

    public void ClearArtistDetail() => _artistDetailPane.Children.Clear();

    private FrameworkElement BuildSearchRowVisual(SearchResultRow row) => row switch
    {
        SearchHeaderRow header => new TextBlock
        {
            Text = header.Title,
            FontSize = 12,
            FontWeight = FontWeights.ExtraBold,
            CharacterSpacing = 40,
            Foreground = Secondary,
            Margin = new Thickness(8, 18, 8, 4),
        },
        SearchMessageRow message => new TextBlock
        {
            Text = message.Text,
            Foreground = Secondary,
            TextWrapping = TextWrapping.Wrap,
            Margin = new Thickness(8, 8, 8, 8),
        },
        ArtResultRow item => BuildResultButton(item),
        _ => throw new ArgumentOutOfRangeException(nameof(row), row, "Unknown search result row"),
    };

    // Whether the dropdown currently holds any rows — the window re-shows it on
    // focus only when a search has produced results (or a status message).
    public bool HasResults => _searchResults.Count > 0;

    // Fill the dropdown from the store's cover-attached results: a status message
    // when the search failed or found nothing, otherwise the sectioned rows. The
    // window opens the dropdown after this returns.
    public void RenderSearchResults(LibrarySearchResults? results, string? error)
    {
        _searchResults.Clear();
        if (error is not null || results is null)
        {
            _searchResults.Add(new SearchMessageRow(error ?? Loc.Chrome("search.failed")));
            return;
        }

        if (results.Albums.Count == 0 && results.Artists.Count == 0 && results.Tracks.Count == 0
            && results.Composers.Count == 0 && results.Works.Count == 0)
        {
            _searchResults.Add(new SearchMessageRow(Loc.Chrome("search.no_matches")));
            return;
        }

        AddSearchSection(Loc.Chrome("search.section.albums"), results.Albums, album =>
            new ArtResultRow(album, album.Title, AlbumSubtitle(album), null, () => _openAlbum(album.Id, null, null)));
        AddSearchSection(Loc.Chrome("search.section.artists"), results.Artists, artist =>
            new ArtResultRow(
                artist, artist.Name, artist.AlbumCountText, null, () => ShowArtistDetail(artist.ArtistId)));
        AddSearchSection(Loc.Chrome("search.section.tracks"), results.Tracks, track =>
            new ArtResultRow(
                track, track.Title, $"{track.ArtistName} — {track.AlbumTitle}", track.DurationLabel,
                () => _openAlbum(track.AlbumId, null, null)));
        AddSearchSection(Loc.Chrome("search.section.composers"), results.Composers, composer =>
            new ArtResultRow(
                composer, composer.Name, composer.WorkCountText, null, () => ShowComposerDetail(composer.ArtistId)));
        AddSearchSection(Loc.Chrome("search.section.works"), results.Works, work =>
            new ArtResultRow(
                work, work.Title, work.ComposerNames ?? string.Empty, null, () => ShowWorkDetail(work.WorkId)));
    }

    // "artist (year)" when the album carries a year, otherwise just the artist.
    private static string AlbumSubtitle(Album album) =>
        string.IsNullOrEmpty(album.YearText) ? album.Artist : $"{album.Artist} ({album.YearText})";

    private void AddSearchSection<T>(
        string title,
        IReadOnlyList<T> rows,
        Func<T, SearchResultRow> project)
    {
        if (rows.Count == 0)
        {
            return;
        }

        _searchResults.Add(new SearchHeaderRow(title));
        foreach (var row in rows)
        {
            _searchResults.Add(project(row));
        }
    }

    // A square cover slot bound one-way to the source's cover property (usually
    // "Cover") so it fills in as the async load lands (and detaches when the row
    // is recycled). Placeholder fill stands in until then and when there is none.
    private static Border BuildArtLeading(object coverSource, double size = 46, double radius = 8, string coverPath = "Cover")
    {
        var image = new Image { Stretch = Stretch.UniformToFill };
        image.SetBinding(Image.SourceProperty, new Binding
        {
            Source = coverSource,
            Path = new PropertyPath(coverPath),
            Mode = BindingMode.OneWay,
        });
        return new Border
        {
            Width = size,
            Height = size,
            CornerRadius = new CornerRadius(radius),
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = image,
        };
    }

    // Assemble one result row: the 46x46 cover slot, a title/subtitle column, and
    // an optional trailing label (a track's duration). The whole row is a rounded
    // button whose default pointer-over supplies the hover fill; a click dismisses
    // the dropdown, then runs the row's navigation.
    private Button BuildResultButton(ArtResultRow row)
    {
        var (title, subtitle, trailing, action) = (row.Title, row.Subtitle, row.Trailing, row.Action);
        var leading = BuildArtLeading(row.CoverSource);

        var text = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(new TextBlock
        {
            Text = title,
            FontSize = 16,
            FontWeight = FontWeights.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        });
        if (!string.IsNullOrWhiteSpace(subtitle))
        {
            text.Children.Add(new TextBlock
            {
                Text = subtitle,
                FontSize = 13,
                FontWeight = FontWeights.Medium,
                Foreground = Secondary,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
            });
        }

        var grid = new Grid { ColumnSpacing = 12, VerticalAlignment = VerticalAlignment.Center };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(leading, 0);
        Grid.SetColumn(text, 1);
        grid.Children.Add(leading);
        grid.Children.Add(text);
        if (!string.IsNullOrWhiteSpace(trailing))
        {
            var duration = new TextBlock
            {
                Text = trailing,
                FontSize = 14,
                Foreground = Secondary,
                VerticalAlignment = VerticalAlignment.Center,
            };
            Grid.SetColumn(duration, 2);
            grid.Children.Add(duration);
        }

        var button = new Button
        {
            Content = grid,
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(8, 6, 8, 6),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            button, string.IsNullOrWhiteSpace(subtitle) ? title : $"{title}, {subtitle}");
        button.Click += async (_, _) =>
        {
            _dismissSearch();
            await action();
        };
        return button;
    }

    private static Brush Secondary => (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];

}
