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

    // The leading 46x46 cover slot, bound one-way to the source's "Cover" so it
    // fills in as the async load lands (and detaches when the row is recycled).
    private static Border BuildArtLeading(object coverSource)
    {
        var image = new Image { Stretch = Stretch.UniformToFill };
        image.SetBinding(Image.SourceProperty, new Binding
        {
            Source = coverSource,
            Path = new PropertyPath("Cover"),
            Mode = BindingMode.OneWay,
        });
        return new Border
        {
            Width = 46,
            Height = 46,
            CornerRadius = new CornerRadius(8),
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
        _composerDetailPane.Children.Add(new TextBlock
        {
            Text = detail.Composer.Name,
            Style = (Style)Application.Current.Resources["TitleTextBlockStyle"],
        });
        if (detail.WorkGroups.Count > 0)
        {
            AddPaneHeader(_composerDetailPane, Loc.Chrome("search.section.works"));
            foreach (var group in detail.WorkGroups)
            {
                if (group.Parent is not null)
                {
                    _composerDetailPane.Children.Add(WorkButton(group.Parent));
                }
                foreach (var work in group.Works)
                {
                    _composerDetailPane.Children.Add(WorkButton(work));
                }
            }
        }
        if (detail.UnlinkedReleaseRoles.Count > 0)
        {
            AddPaneHeader(_composerDetailPane, Loc.Chrome("search.section.credits"));
            foreach (var role in detail.UnlinkedReleaseRoles)
            {
                _composerDetailPane.Children.Add(PaneButton(role.AlbumTitle, role.SourceCredit ?? string.Empty, () =>
                    _openAlbum(role.AlbumId, null, null)));
            }
        }
        if (detail.UnlinkedTrackRoles.Count > 0)
        {
            AddPaneHeader(_composerDetailPane, Loc.Chrome("search.section.recordings"));
            foreach (var role in detail.UnlinkedTrackRoles)
            {
                _composerDetailPane.Children.Add(PaneButton(role.TrackTitle, role.AlbumTitle, () => _openAlbum(role.AlbumId, null, null)));
            }
        }
        if (detail.DefaultWorkId is not null)
        {
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

    private Button WorkButton(WorkSummary work) =>
        PaneButton(work.Title, work.ComposerNames ?? string.Empty, () => ShowWorkDetail(work.WorkId));

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
        _composerDetailPane.Children.Add(new TextBlock
        {
            Text = detail.Work.Title,
            Style = (Style)Application.Current.Resources["TitleTextBlockStyle"],
        });
        if (detail.ChildWorks.Count > 0)
        {
            AddPaneHeader(_composerDetailPane, Loc.Chrome("search.section.works"));
            foreach (var work in detail.ChildWorks)
            {
                _composerDetailPane.Children.Add(WorkButton(work));
            }
        }
        if (detail.Releases.Count > 0)
        {
            AddPaneHeader(_composerDetailPane, Loc.Chrome("search.section.releases"));
            foreach (var release in detail.Releases)
            {
                _composerDetailPane.Children.Add(PaneButton(release.AlbumTitle, release.DisplaySubtitle, () =>
                    _openAlbum(release.AlbumId, null, release.ReleaseId)));
            }
        }
        if (detail.Tracks.Count > 0)
        {
            AddPaneHeader(_composerDetailPane, Loc.Chrome("search.section.recordings"));
            foreach (var track in detail.Tracks)
            {
                _composerDetailPane.Children.Add(PaneButton(track.TrackTitle, track.AlbumTitle, () => _openAlbum(track.AlbumId, null, null)));
            }
        }
    }
}
