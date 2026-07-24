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

// The composer, artist, and work detail panes: their headers, section rows,
// and row-building helpers. Split out of BrowserPanes.cs unchanged.
internal sealed partial class BrowserPanes
{
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
        detail.Composer.AttachCover(_mediaPaths, _dispatcher);
        foreach (var group in detail.WorkGroups)
        {
            if (group.Parent is not null)
            {
                group.Parent.AttachCover(_mediaPaths, _dispatcher);
            }
            foreach (var work in group.Works)
            {
                work.AttachCover(_mediaPaths, _dispatcher);
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
        detail.Artist.AttachCover(_mediaPaths, _dispatcher);
        foreach (var album in detail.Albums)
        {
            album.AttachCover(_mediaPaths, _dispatcher);
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
        detail.Work.AttachCover(_mediaPaths, _dispatcher);
        foreach (var work in detail.ChildWorks)
        {
            work.AttachCover(_mediaPaths, _dispatcher);
        }
        foreach (var release in detail.Releases)
        {
            release.AttachCover(_mediaPaths, _dispatcher);
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
