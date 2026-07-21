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
// read the current handle for their fetches (composer/work/artist detail) and
// covers; the window owns pane visibility and the status line, passed in as
// callbacks. Opening an album is injected because the album-detail dialog
// lives on the window.
internal sealed class BrowserPanes
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

    private readonly SessionStore _session;
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
        SessionStore session,
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
        _session = session;
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

    public async System.Threading.Tasks.Task ShowComposerDetail(string artistId)
    {
        var (current, response) = await _session.RunForCurrentHandle(
            handle => NativeBae.GetComposerDetail(handle, artistId));
        if (!current)
        {
            return;
        }
        if (response.Error is not null || response.Detail is null)
        {
            _setStatus(response.Error ?? Loc.Chrome("composer.open_failed"));
            return;
        }

        var detail = response.Detail;
        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }
        detail.Composer.AttachCover(handle, _dispatcher);
        foreach (var group in detail.WorkGroups)
        {
            if (group.Parent is not null)
            {
                group.Parent.AttachCover(handle, _dispatcher);
            }
            foreach (var work in group.Works)
            {
                work.AttachCover(handle, _dispatcher);
            }
        }

        _showComposerBrowser();
        _composerDetailPane.Children.Clear();
        _composerDetailPane.Children.Add(ComposerHeader(detail.Composer));
        if (detail.WorkGroups.Count > 0)
        {
            var works = DetailSection(Loc.Chrome("search.section.works"));
            foreach (var group in detail.WorkGroups)
            {
                if (group.Parent is not null)
                {
                    works.Children.Add(WorkRow(group.Parent));
                }
                foreach (var work in group.Works)
                {
                    works.Children.Add(WorkRow(work));
                }
            }
            _composerDetailPane.Children.Add(works);
        }
        if (detail.UnlinkedReleaseRoles.Count > 0)
        {
            var credits = DetailSection(Loc.Chrome("search.section.credits"));
            foreach (var role in detail.UnlinkedReleaseRoles)
            {
                credits.Children.Add(DetailTextRow(role.AlbumTitle, role.SourceCredit, () => _openAlbum(role.AlbumId, null, null)));
            }
            _composerDetailPane.Children.Add(credits);
        }
        if (detail.UnlinkedTrackRoles.Count > 0)
        {
            var recordings = DetailSection(Loc.Chrome("search.section.recordings"));
            foreach (var role in detail.UnlinkedTrackRoles)
            {
                recordings.Children.Add(DetailTextRow(role.TrackTitle, role.AlbumTitle, () => _openAlbum(role.AlbumId, null, null)));
            }
            _composerDetailPane.Children.Add(recordings);
        }
        if (detail.DefaultWorkId is not null)
        {
            _composerDetailPane.Children.Add(DetailDivider());
            await ShowWorkDetail(detail.DefaultWorkId, replacePane: false);
        }
    }

    public async System.Threading.Tasks.Task ShowArtistDetail(string artistId)
    {
        var (current, response) = await _session.RunForCurrentHandle(
            handle => NativeBae.GetArtistDetail(handle, artistId));
        if (!current)
        {
            return;
        }
        if (response.Error is not null || response.Detail is null)
        {
            _setStatus(response.Error ?? Loc.Chrome("artist.open_failed"));
            return;
        }

        var detail = response.Detail;
        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }
        detail.Artist.AttachCover(handle, _dispatcher);
        foreach (var album in detail.Albums)
        {
            album.AttachCover(handle, _dispatcher);
        }

        _showArtistBrowser();
        _artistDetailPane.Children.Clear();
        _artistDetailPane.Children.Add(new TextBlock
        {
            Text = detail.Artist.Name,
            Style = (Style)Application.Current.Resources["TitleTextBlockStyle"],
        });
        _artistDetailPane.Children.Add(new TextBlock
        {
            Text = detail.Artist.AlbumCountText,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
        });
        if (detail.Albums.Count > 0)
        {
            AddPaneHeader(_artistDetailPane, Loc.Chrome("search.section.albums"));
            foreach (var album in detail.Albums)
            {
                _artistDetailPane.Children.Add(PaneButton(
                    album.Title,
                    album.Year?.ToString() ?? string.Empty,
                    () => _openAlbum(album.Id, null, null)));
            }
        }
    }

    // The composer/work detail column. The composer header pairs a 72x72 cover
    // slot with the name and work count; each section is a bold label above its
    // rows; a hairline separates the works list from the selected-work detail.

    private Button WorkRow(WorkSummary work) =>
        DetailArtRow(work, "Cover", work.Title, work.ComposerNames, () => ShowWorkDetail(work.WorkId));

    private static Grid ComposerHeader(ComposerSummary composer)
    {
        var art = BuildArtLeading(composer, 72, 8);
        var text = new StackPanel { Spacing = 5, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(new TextBlock
        {
            Text = composer.Name,
            FontSize = 22,
            FontWeight = FontWeights.Bold,
            CharacterSpacing = -14,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        });
        text.Children.Add(new TextBlock
        {
            Text = composer.WorkCountText,
            FontSize = 13,
            Foreground = Secondary,
        });
        var grid = new Grid { ColumnSpacing = 16, VerticalAlignment = VerticalAlignment.Center };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(art, 0);
        Grid.SetColumn(text, 1);
        grid.Children.Add(art);
        grid.Children.Add(text);
        return grid;
    }

    private static TextBlock WorkDetailTitle(string title) => new()
    {
        Text = title,
        FontSize = 18,
        FontWeight = FontWeights.Bold,
        CharacterSpacing = -11,
        MaxLines = 1,
        TextTrimming = TextTrimming.CharacterEllipsis,
    };

    // A section is a bold label above its rows, added to the pane as one child so
    // the pane's spacing separates whole sections while rows stay tight.
    private static StackPanel DetailSection(string label)
    {
        var section = new StackPanel { Spacing = 2 };
        section.Children.Add(new TextBlock { Text = label, FontSize = 15, FontWeight = FontWeights.Bold });
        return section;
    }

    private static Border DetailDivider() => new()
    {
        Height = 1,
        HorizontalAlignment = HorizontalAlignment.Stretch,
        Background = (Brush)Application.Current.Resources["DividerStrokeColorDefaultBrush"],
    };

    // A row with a 42x42 cover slot, a title line, and an optional subtitle,
    // extended edge-to-edge so its hover fill bleeds past the content padding.
    private static Button DetailArtRow(
        object coverSource, string coverPath, string title, string? subtitle, Func<System.Threading.Tasks.Task> action)
    {
        var leading = BuildArtLeading(coverSource, 42, 6, coverPath);
        var text = DetailRowText(title, subtitle);
        var grid = new Grid { ColumnSpacing = 12, VerticalAlignment = VerticalAlignment.Center };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(leading, 0);
        Grid.SetColumn(text, 1);
        grid.Children.Add(leading);
        grid.Children.Add(text);
        return DetailRowButton(grid, RowAutomationName(title, subtitle), action);
    }

    private static Button DetailTextRow(string title, string? subtitle, Func<System.Threading.Tasks.Task> action) =>
        DetailRowButton(DetailRowText(title, subtitle), RowAutomationName(title, subtitle), action);

    private static StackPanel DetailRowText(string title, string? subtitle)
    {
        var text = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
        text.Children.Add(new TextBlock
        {
            Text = title,
            FontSize = 14,
            FontWeight = FontWeights.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        });
        if (!string.IsNullOrWhiteSpace(subtitle))
        {
            text.Children.Add(new TextBlock
            {
                Text = subtitle,
                FontSize = 11.5,
                Foreground = Secondary,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
            });
        }
        return text;
    }

    private static Button DetailRowButton(FrameworkElement content, string automationName, Func<System.Threading.Tasks.Task> action)
    {
        var button = new Button
        {
            Content = content,
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(8, 6, 8, 6),
            Margin = new Thickness(-8, 0, -8, 0),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, automationName);
        button.Click += async (_, _) => await action();
        return button;
    }

    private static string RowAutomationName(string title, string? subtitle) =>
        string.IsNullOrWhiteSpace(subtitle) ? title : $"{title}, {subtitle}";

    private void AddPaneHeader(StackPanel pane, string title)
    {
        pane.Children.Add(new TextBlock
        {
            Text = title,
            Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"],
        });
    }

    private static Button PaneButton(string title, string subtitle, Func<System.Threading.Tasks.Task> action)
    {
        var panel = new StackPanel { Spacing = 2 };
        panel.Children.Add(new TextBlock { Text = title, MaxLines = 1, TextTrimming = TextTrimming.CharacterEllipsis });
        if (!string.IsNullOrWhiteSpace(subtitle))
        {
            panel.Children.Add(new TextBlock
            {
                Text = subtitle,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            });
        }
        var button = new Button
        {
            Content = panel,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
        };
        button.Click += async (_, _) => await action();
        return button;
    }

    private async System.Threading.Tasks.Task ShowWorkDetail(string workId, bool replacePane = true)
    {
        var (current, response) = await _session.RunForCurrentHandle(
            handle => NativeBae.GetWorkDetail(handle, workId));
        if (!current)
        {
            return;
        }
        if (response.Error is not null || response.Detail is null)
        {
            _setStatus(response.Error ?? Loc.Chrome("work.open_failed"));
            return;
        }

        var detail = response.Detail;
        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return;
        }
        detail.Work.AttachCover(handle, _dispatcher);
        foreach (var work in detail.ChildWorks)
        {
            work.AttachCover(handle, _dispatcher);
        }
        foreach (var release in detail.Releases)
        {
            release.AttachCover(handle, _dispatcher);
        }

        _showComposerBrowser();
        if (replacePane)
        {
            _composerDetailPane.Children.Clear();
        }
        _composerDetailPane.Children.Add(WorkDetailTitle(detail.Work.Title));
        if (detail.ChildWorks.Count > 0)
        {
            var works = DetailSection(Loc.Chrome("search.section.works"));
            foreach (var work in detail.ChildWorks)
            {
                works.Children.Add(WorkRow(work));
            }
            _composerDetailPane.Children.Add(works);
        }
        if (detail.Releases.Count > 0)
        {
            var releases = DetailSection(Loc.Chrome("search.section.releases"));
            foreach (var release in detail.Releases)
            {
                releases.Children.Add(DetailArtRow(
                    release, "CoverImage", release.AlbumTitle, release.DisplaySubtitle,
                    () => _openAlbum(release.AlbumId, null, release.ReleaseId)));
            }
            _composerDetailPane.Children.Add(releases);
        }
        if (detail.Tracks.Count > 0)
        {
            var recordings = DetailSection(Loc.Chrome("search.section.recordings"));
            foreach (var track in detail.Tracks)
            {
                recordings.Children.Add(DetailTextRow(track.TrackTitle, track.AlbumTitle, () => _openAlbum(track.AlbumId, null, null)));
            }
            _composerDetailPane.Children.Add(recordings);
        }
    }
}
