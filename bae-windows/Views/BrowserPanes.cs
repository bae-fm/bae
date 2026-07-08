using System;
using System.Collections.Generic;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Renders the two library panes that aren't the album grid: the search-results
// list and the composer/work detail column. Both read the current handle for
// their fetches (composer/work detail) and covers; the window owns pane
// visibility and the status line, passed in as callbacks. Opening an album is
// injected because the album-detail dialog lives on the window.
internal sealed class BrowserPanes
{
    private readonly SessionStore _session;
    private readonly DispatcherQueue _dispatcher;
    private readonly StackPanel _searchResultsPanel;
    private readonly StackPanel _composerDetailPane;
    private readonly Action<string> _setStatus;
    private readonly Action _showComposerBrowser;
    private readonly Func<string, string?, string?, System.Threading.Tasks.Task> _openAlbum;

    public BrowserPanes(
        SessionStore session,
        DispatcherQueue dispatcher,
        StackPanel searchResultsPanel,
        StackPanel composerDetailPane,
        Action<string> setStatus,
        Action showComposerBrowser,
        Func<string, string?, string?, System.Threading.Tasks.Task> openAlbum)
    {
        _session = session;
        _dispatcher = dispatcher;
        _searchResultsPanel = searchResultsPanel;
        _composerDetailPane = composerDetailPane;
        _setStatus = setStatus;
        _showComposerBrowser = showComposerBrowser;
        _openAlbum = openAlbum;
    }

    public void ClearComposerDetail() => _composerDetailPane.Children.Clear();

    // Render the search-results list from the store's cover-attached results. The
    // window has already flipped to the search pane; this fills the panel and sets
    // the status line (failure, no matches, or clear on results).
    public void RenderSearchResults(LibrarySearchResults? results, string? error)
    {
        _searchResultsPanel.Children.Clear();
        if (error is not null || results is null)
        {
            _setStatus(error ?? Loc.Chrome("search.failed"));
            return;
        }

        if (results.Albums.Count == 0 && results.Tracks.Count == 0
            && results.Composers.Count == 0 && results.Works.Count == 0)
        {
            _setStatus(Loc.Chrome("search.no_matches"));
            return;
        }

        _setStatus(string.Empty);
        AddSearchSection(Loc.Chrome("search.section.albums"), results.Albums, album =>
            SearchButton(album.Title, album.Artist, () => _openAlbum(album.Id, null, null)));
        AddSearchSection(Loc.Chrome("search.section.tracks"), results.Tracks, track =>
            SearchButton(track.Title, $"{track.ArtistName} — {track.AlbumTitle}", () => _openAlbum(track.AlbumId, null, null)));
        AddSearchSection(Loc.Chrome("search.section.composers"), results.Composers, composer =>
            SearchButton(composer.Name, composer.WorkCountText, () => ShowComposerDetail(composer.ArtistId)));
        AddSearchSection(Loc.Chrome("search.section.works"), results.Works, work =>
            SearchButton(work.Title, work.ComposerNames ?? string.Empty, () => ShowWorkDetail(work.WorkId)));
    }

    private void AddSearchSection<T>(
        string title,
        IReadOnlyList<T> rows,
        Func<T, Button> button)
    {
        if (rows.Count == 0)
        {
            return;
        }

        _searchResultsPanel.Children.Add(new TextBlock
        {
            Text = title,
            Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"],
        });
        foreach (var row in rows)
        {
            _searchResultsPanel.Children.Add(button(row));
        }
    }

    private static Button SearchButton(string title, string subtitle, Func<System.Threading.Tasks.Task> action)
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
            AddPaneHeader(Loc.Chrome("search.section.works"));
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
            AddPaneHeader(Loc.Chrome("search.section.credits"));
            foreach (var role in detail.UnlinkedReleaseRoles)
            {
                _composerDetailPane.Children.Add(PaneButton(role.AlbumTitle, role.SourceCredit ?? string.Empty, () =>
                    _openAlbum(role.AlbumId, null, null)));
            }
        }
        if (detail.UnlinkedTrackRoles.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.recordings"));
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

    private Button WorkButton(WorkSummary work) =>
        PaneButton(work.Title, work.ComposerNames ?? string.Empty, () => ShowWorkDetail(work.WorkId));

    private void AddPaneHeader(string title)
    {
        _composerDetailPane.Children.Add(new TextBlock
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
            AddPaneHeader(Loc.Chrome("search.section.works"));
            foreach (var work in detail.ChildWorks)
            {
                _composerDetailPane.Children.Add(WorkButton(work));
            }
        }
        if (detail.Releases.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.releases"));
            foreach (var release in detail.Releases)
            {
                _composerDetailPane.Children.Add(PaneButton(release.AlbumTitle, release.DisplaySubtitle, () =>
                    _openAlbum(release.AlbumId, null, release.ReleaseId)));
            }
        }
        if (detail.Tracks.Count > 0)
        {
            AddPaneHeader(Loc.Chrome("search.section.recordings"));
            foreach (var track in detail.Tracks)
            {
                _composerDetailPane.Children.Add(PaneButton(track.TrackTitle, track.AlbumTitle, () => _openAlbum(track.AlbumId, null, null)));
            }
        }
    }
}
