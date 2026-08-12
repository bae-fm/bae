using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Templates;
using Avalonia.Data;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Threading;

namespace Bae.Desktop;

// The inline album-detail expansion: the full-width panel shown under the grid row
// that holds the expanded album (the macOS AlbumExpansionSlot / AlbumDetailView
// analogue). It shows the cover, the album's releases (with a picker when there's
// more than one), play / shuffle / queue actions, the track list, and the total
// duration. The per-release metadata dialogs (edit, re-identify, change cover,
// gallery, export, delete) and the storage band are the album-detail dialog family
// and land with that area; the panel grows those actions then. The browser's
// album-detail store owns the live query; this view renders its current value.
internal static class AlbumExpansionView
{
    public static Task<Control?> BuildAsync(
        AppService app,
        ReleaseActionDialogs dialogs,
        Album card,
        Action onClose)
    {
        var host = new ContentControl
        {
            Content = new ProgressBar { IsIndeterminate = true },
        };
        var missingReported = false;
        void Render()
        {
            var store = app.AlbumDetailStore;
            if (store.AlbumId != card.Id || !store.HasValue)
            {
                return;
            }
            if (store.Detail is not { } detail)
            {
                host.Content = new Panel();
                if (!missingReported)
                {
                    missingReported = true;
                    app.ShowError(Loc.Chrome("error.title"), Loc.Chrome("album.open_failed"));
                }
                return;
            }
            host.Content = BuildContent(app, dialogs, card, detail, onClose);
        }

        app.AlbumDetailStore.Changed += Render;
        host.DetachedFromVisualTree += (_, _) =>
        {
            app.AlbumDetailStore.Changed -= Render;
            app.AlbumDetailStore.Clear(card.Id);
        };
        app.AlbumDetailStore.Select(card.Id);
        Render();
        return Task.FromResult<Control?>(host);
    }

    private static Control BuildContent(
        AppService app,
        ReleaseActionDialogs dialogs,
        Album card,
        AlbumDetail detail,
        Action onClose)
    {
        if (detail.Releases.Count == 0)
        {
            app.ShowError(Loc.Chrome("error.title"), Loc.Chrome("album.open_failed"));
            return new Panel();
        }

        // Attach media paths to every release so its own cover loads off the UI
        // thread through the same (id, version) cache the grid tiles use.
        foreach (var release in detail.Releases)
        {
            release.AttachCover(app.Images, Dispatcher.UIThread);
        }

        // The release the panel acts on: an explicitly-primary release, else the
        // first. The picker (added below when there's more than one) reassigns it.
        var selectedRelease = detail.Releases.FirstOrDefault(r => r.ReleaseId == detail.PrimaryReleaseId)
            ?? detail.Releases[0];

        // ── Actions ───────────────────────────────────────────────────────────
        var playButton = PrimaryButton(Loc.Chrome("action.play"));
        playButton.Click += (_, _) => PlayRelease(app, selectedRelease, shuffle: false);
        var shuffleButton = new Button { Content = Loc.Chrome("album.shuffle") };
        shuffleButton.Click += (_, _) => PlayRelease(app, selectedRelease, shuffle: true);
        var editButton = new Button { Content = Loc.Chrome("album.edit.label") };
        editButton.Click += async (_, _) => await dialogs.ShowEditMetadata(selectedRelease.ReleaseId);
        var reidentifyButton = new Button { Content = Loc.Chrome("album.reidentify.label") };
        reidentifyButton.Click += async (_, _) => await dialogs.ShowReidentify(selectedRelease.ReleaseId, detail.Artist, detail.Title);

        var moreButton = new Button { Content = "⋯" };
        var playNext = new MenuItem { Header = Loc.Chrome("menu.play_next") };
        playNext.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                app.Queue.AddReleaseNext(selectedRelease.ReleaseId);
            }
        };
        var addQueue = new MenuItem { Header = Loc.Chrome("menu.add_to_queue") };
        addQueue.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                app.Queue.AddReleaseToQueue(selectedRelease.ReleaseId);
            }
        };
        var changeCover = new MenuItem { Header = Loc.Chrome("menu.change_cover") };
        changeCover.Click += async (_, _) => await dialogs.ShowChangeCover(
            selectedRelease.ReleaseId,
            selectedRelease.ImageFiles);

        var moreItems = new List<MenuItem> { playNext, addQueue };
        // Set-primary only means something when the album has more than one pressing.
        if (detail.Releases.Count > 1)
        {
            var setPrimary = new MenuItem { Header = Loc.Chrome("menu.set_primary") };
            setPrimary.Click += async (_, _) =>
            {
                var (current, error) = await app.ReleaseEditor.SetPrimaryRelease(detail.Id, selectedRelease.ReleaseId);
                if (current && error is not null)
                {
                    app.ShowError(Loc.Chrome("error.title"), error);
                }
            };
            moreItems.Add(setPrimary);
        }
        moreItems.Add(changeCover);
        moreButton.Flyout = new MenuFlyout { ItemsSource = moreItems };

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(playButton);
        actions.Children.Add(shuffleButton);
        actions.Children.Add(editButton);
        actions.Children.Add(reidentifyButton);
        actions.Children.Add(moreButton);

        // ── Track list ────────────────────────────────────────────────────────
        var trackList = new StackPanel { Spacing = 0 };
        void RenderTracks()
        {
            trackList.Children.Clear();
            foreach (var track in selectedRelease.Tracks)
            {
                var captured = track;
                trackList.Children.Add(AlbumExpansionRows.BuildTrackRow(
                    captured.PositionLabel,
                    captured.Title,
                    captured.DisplayArtist,
                    captured.DurationLabel,
                    onPlay: () => PlayFromTrack(app, selectedRelease, captured),
                    onPlayNext: () => QueueTrack(app, captured, next: true),
                    onAddToQueue: () => QueueTrack(app, captured, next: false)));
            }
        }
        RenderTracks();

        var totalDuration = new TextBlock { FontSize = 12, Opacity = 0.7 };
        totalDuration[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        void RenderTotalDuration()
        {
            totalDuration.Text = selectedRelease.TotalDurationLabel;
            totalDuration.IsVisible = !string.IsNullOrEmpty(totalDuration.Text);
        }
        RenderTotalDuration();

        // ── Large cover (rebinds to the selected release, falling back to the card
        // cover when the release carries none) ─────────────────────────────────
        var coverImage = new Image { Stretch = Stretch.UniformToFill };
        void RebindCover() => coverImage[!Image.SourceProperty] = new Binding(
            selectedRelease.HasOwnCover ? nameof(Release.Cover) : nameof(Album.Cover))
        {
            Source = selectedRelease.HasOwnCover ? selectedRelease : (object)card,
            Mode = Avalonia.Data.BindingMode.OneWay,
        };
        RebindCover();

        var detailStack = new StackPanel { Spacing = 12 };
        detailStack.Children.Add(AlbumExpansionRows.BuildHeaderBlock(detail.Title, detail.Artist));
        if (detail.Releases.Count > 1)
        {
            var picker = new ComboBox
            {
                ItemsSource = detail.Releases,
                SelectedItem = selectedRelease,
                ItemTemplate = new FuncDataTemplate<Release>((release, _) =>
                    new TextBlock { Text = release?.DisplayName ?? string.Empty }),
            };
            picker.SelectionChanged += (_, _) =>
            {
                if (picker.SelectedItem is Release release)
                {
                    selectedRelease = release;
                    RebindCover();
                    RenderTracks();
                    RenderTotalDuration();
                }
            };
            detailStack.Children.Add(picker);
        }
        detailStack.Children.Add(actions);
        detailStack.Children.Add(trackList);
        detailStack.Children.Add(totalDuration);

        var coverBorder = new Border
        {
            Width = 260,
            Height = 260,
            CornerRadius = new CornerRadius(14),
            ClipToBounds = true,
            VerticalAlignment = VerticalAlignment.Top,
            Child = coverImage,
        };
        coverBorder[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        // A click on the cover opens the release's gallery in the lightbox.
        coverBorder.PointerPressed += (_, _) => dialogs.ShowGallery(selectedRelease.ReleaseId, selectedRelease.GalleryItems);

        var contentGrid = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*"), ColumnSpacing = 28 };
        Grid.SetColumn(coverBorder, 0);
        Grid.SetColumn(detailStack, 1);
        contentGrid.Children.Add(coverBorder);
        contentGrid.Children.Add(detailStack);

        var panelCard = new Border
        {
            CornerRadius = new CornerRadius(18),
            Padding = new Thickness(24),
            Margin = new Thickness(0, 8),
            BorderThickness = new Thickness(1),
            Child = contentGrid,
        };
        panelCard[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");
        panelCard[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");

        // Close affordance, top-trailing — mirroring the macOS ✕; clears the
        // expanded album, which collapses this panel.
        var closeButton = new Button
        {
            Content = "✕",
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(8),
            CornerRadius = new CornerRadius(9),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(0, 16, 16, 0),
        };
        closeButton[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        Avalonia.Automation.AutomationProperties.SetName(closeButton, Loc.Chrome("action.close"));
        closeButton.Click += (_, _) => onClose();

        return new Panel { Children = { panelCard, closeButton } };
    }

    private static Button PrimaryButton(string text)
    {
        var button = new Button { Content = text };
        button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeOnAccentBrush");
        return button;
    }

    private static void PlayRelease(AppService app, Release release, bool shuffle)
    {
        if (!string.IsNullOrEmpty(release.ReleaseId))
        {
            app.Playback.PlayRelease(release.ReleaseId, -1, shuffle);
        }
    }

    private static void PlayFromTrack(AppService app, Release release, Track track)
    {
        if (!string.IsNullOrEmpty(release.ReleaseId))
        {
            app.Playback.PlayRelease(release.ReleaseId, release.Tracks.IndexOf(track), false);
        }
    }

    private static void QueueTrack(AppService app, Track track, bool next)
    {
        var (current, error) = next
            ? app.Queue.AddNext(new[] { track.TrackId })
            : app.Queue.AddToQueue(new[] { track.TrackId });
        if (current && error is not null)
        {
            app.ShowError(Loc.Chrome("error.title"), error);
        }
    }
}
