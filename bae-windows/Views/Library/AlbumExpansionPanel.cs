using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using uniffi.bae_bridge;
using Windows.System;

namespace Bae.Windows;

// The inline album-detail expansion: the full-width panel injected under the grid
// row that holds the expanded album, replacing the old modal AlbumDetailDialog.
// It shows the cover, the album's releases (with a picker when there's more than
// one), the primary actions, the track list, and the storage band. The macOS
// analogue is AlbumExpansionContent; the content is otherwise the dialog's,
// minus the modal chrome — because the panel is not a ContentDialog, the
// per-release action dialogs (edit / re-identify / gallery / change cover /
// export / delete) open directly instead of the dialog's hide-then-dispatch
// dance, and errors go to the window status line, which the panel no longer
// occludes. Built per open (BuildAsync); Dispose tears down the storage-band
// live-refresh registrations.
internal sealed partial class AlbumExpansionPanel : IDisposable
{
    private readonly SessionStore _session;
    private readonly Microsoft.UI.Dispatching.DispatcherQueue _dispatcher;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly Action<string> _setStatus;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly StorageStore _storage;
    private readonly TransferProgressStore _transfers;
    private readonly ProjectionRegistry _projections;
    private readonly Action _onClose;

    // The storage-band live-refresh hooks, torn down on Dispose (collapse / mode
    // switch / library teardown): the Release/Album invalidation registrations and
    // the transfer-progress subscription.
    private readonly List<IDisposable> _registrations = new();
    private Action? _onTransfersChanged;
    private bool _disposed;

    public AlbumExpansionPanel(
        SessionStore session,
        Microsoft.UI.Dispatching.DispatcherQueue dispatcher,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        Action<string> setStatus,
        ReleaseActionDialogs releaseActions,
        StorageStore storage,
        TransferProgressStore transfers,
        ProjectionRegistry projections,
        Action onClose)
    {
        _session = session;
        _dispatcher = dispatcher;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _setStatus = setStatus;
        _releaseActions = releaseActions;
        _storage = storage;
        _transfers = transfers;
        _projections = projections;
        _onClose = onClose;
    }

    // Fetch the album's detail and build the panel. Returns null when the session
    // closed mid-fetch or the album can't be opened (the caller drops a null and
    // leaves the row collapsed). scrollToTrackId, when set, is the "go to now
    // playing" reveal target: the panel starts on the release that contains it and
    // flashes its row. initialReleaseId picks a starting release (a work-detail
    // "go to release" jump).
    public async System.Threading.Tasks.Task<FrameworkElement?> BuildAsync(
        Album card,
        string? scrollToTrackId,
        string? initialReleaseId)
    {
        var (current, response) = await _session.RunForCurrentHandle(
            handle => NativeBae.GetAlbumDetail(handle, card.Id));
        if (!current)
        {
            return null;
        }
        if (response.Error is not null || response.Detail is null)
        {
            _setStatus(response.Error ?? Loc.Chrome("album.open_failed"));
            return null;
        }

        var detail = response.Detail;
        if (detail.Releases.Count == 0)
        {
            _setStatus(Loc.Chrome("album.open_failed"));
            return null;
        }

        // Attach the current handle to every release so its own cover loads off the
        // UI thread through the same (id, version) cache the grid tiles use; the
        // large cover binds to the selected release's.
        if (_session.CurrentHandleOrNull() is { } coverHandle)
        {
            foreach (var release in detail.Releases)
            {
                release.AttachCover(coverHandle, _dispatcher);
            }
        }

        // The release the panel acts on: when revealing a now-playing track, the
        // release that actually contains it (which may not be the primary); else
        // an explicitly-picked release, the primary, or the first. The picker
        // (added below when there's more than one) reassigns it, and every
        // per-release action and the track list read it.
        Release? trackRelease = scrollToTrackId is null
            ? null
            : detail.Releases.FirstOrDefault(r => r.Tracks.Any(t => t.TrackId == scrollToTrackId));
        Release selectedRelease = trackRelease
            ?? detail.Releases.FirstOrDefault(r => r.ReleaseId == initialReleaseId)
            ?? detail.Releases.FirstOrDefault(r => r.ReleaseId == detail.PrimaryReleaseId)
            ?? detail.Releases[0];

        // Title + artist. The dialog carried the title in ContentDialog.Title;
        // inline it is a heading in the panel.
        var title = new TextBlock
        {
            Text = detail.Title,
            FontSize = 24,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            MaxLines = 2,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        var artist = new TextBlock
        {
            Text = detail.Artist,
            FontSize = 15,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };

        var playButton = new Button
        {
            Content = Loc.Chrome("action.play"),
            Style = (Style)Application.Current.Resources["AccentButtonStyle"],
        };
        playButton.Click += (_, _) => PlayRelease(selectedRelease, shuffle: false);
        var shuffleButton = new Button { Content = Loc.Chrome("album.shuffle") };
        shuffleButton.Click += (_, _) => PlayRelease(selectedRelease, shuffle: true);
        var editButton = new Button { Content = Loc.Chrome("album.edit.label") };
        editButton.Click += async (_, _) => await _releaseActions.ShowEditMetadata(selectedRelease.ReleaseId);
        var reidentifyButton = new Button { Content = Loc.Chrome("album.reidentify.label") };
        reidentifyButton.Click += async (_, _) =>
            await _releaseActions.ShowReidentify(selectedRelease.ReleaseId, detail.Artist, detail.Title);

        // Overflow menu: queueing, set-primary, export/save, change cover, delete —
        // each opening its own dialog directly (no modal to close first).
        var moreButton = new Button { Content = "⋯" };
        var moreMenu = new MenuFlyout();
        var playNextItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.play_next") };
        playNextItem.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                _session.WithCurrentHandle(
                    handle => NativeBae.AddReleaseNext(handle, selectedRelease.ReleaseId));
            }
        };
        var addQueueItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
        addQueueItem.Click += (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                _session.WithCurrentHandle(
                    handle => NativeBae.AddReleaseToQueue(handle, selectedRelease.ReleaseId));
            }
        };
        var setPrimaryItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.set_primary") };
        setPrimaryItem.Click += async (_, _) =>
        {
            var (setCurrent, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetPrimaryRelease(handle, detail.Id, selectedRelease.ReleaseId));
            if (!setCurrent)
            {
                return;
            }
            _setStatus(error ?? Loc.Chrome("menu.set_primary_done"));
        };
        var changeCoverItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.change_cover") };
        changeCoverItem.Click += async (_, _) =>
            await _releaseActions.ShowChangeCover(detail.Id, selectedRelease.ReleaseId);
        var exportReleaseItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.export") };
        exportReleaseItem.Click += async (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                await _releaseActions.ShowExportRelease(selectedRelease.ReleaseId);
            }
        };
        var saveAsReleaseItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.save_as") };
        saveAsReleaseItem.Click += async (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                await _releaseActions.ShowSaveReleaseAs(selectedRelease.ReleaseId);
            }
        };
        var deleteItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.delete") };
        deleteItem.Click += async (_, _) => await _releaseActions.ConfirmDeleteRelease(selectedRelease.ReleaseId);
        moreMenu.Items.Add(playNextItem);
        moreMenu.Items.Add(addQueueItem);
        if (detail.Releases.Count > 1)
        {
            moreMenu.Items.Add(setPrimaryItem);
        }
        moreMenu.Items.Add(exportReleaseItem);
        moreMenu.Items.Add(saveAsReleaseItem);
        moreMenu.Items.Add(changeCoverItem);
        moreMenu.Items.Add(deleteItem);
        moreButton.Flyout = moreMenu;

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(playButton);
        actions.Children.Add(shuffleButton);
        actions.Children.Add(editButton);
        actions.Children.Add(reidentifyButton);
        actions.Children.Add(moreButton);

        // Track list: click a row to play the release from that track; right-tap
        // for per-track queueing. Inline, the list sizes to content and the outer
        // grid scrolls — its own vertical scroll is disabled so it doesn't cap or
        // nest a second scroller (the dialog's MaxHeight cap is gone).
        var trackList = new ListView
        {
            ItemsSource = selectedRelease.Tracks,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
        };
        ScrollViewer.SetVerticalScrollMode(trackList, ScrollMode.Disabled);
        ScrollViewer.SetVerticalScrollBarVisibility(trackList, ScrollBarVisibility.Disabled);

        void PlayFromTrack(Track track)
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                _session.WithCurrentHandle(
                    handle => NativeBae.PlayRelease(
                        handle, selectedRelease.ReleaseId, selectedRelease.Tracks.IndexOf(track), false));
            }
        }
        void QueueTrack(Track track, bool next)
        {
            var (queueCurrent, error) = _session.WithCurrentHandle(handle => next
                ? NativeBae.AddNext(handle, new[] { track.TrackId })
                : NativeBae.AddToQueue(handle, new[] { track.TrackId }));
            if (queueCurrent && error is not null)
            {
                _setStatus(error);
            }
        }
        trackList.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is Track track)
            {
                PlayFromTrack(track);
            }
        };
        trackList.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element || element.DataContext is not Track track)
            {
                return;
            }
            var menu = new MenuFlyout();
            var play = new MenuFlyoutItem { Text = Loc.Chrome("menu.play") };
            play.Click += (_, _) => PlayFromTrack(track);
            var playNextTrack = new MenuFlyoutItem { Text = Loc.Chrome("menu.play_next") };
            playNextTrack.Click += (_, _) => QueueTrack(track, next: true);
            var addQueueTrack = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
            addQueueTrack.Click += (_, _) => QueueTrack(track, next: false);
            var exportTrack = new MenuFlyoutItem { Text = Loc.Chrome("menu.save_as") };
            exportTrack.Click += async (_, _) => await ExportTrack(track);
            menu.Items.Add(play);
            menu.Items.Add(playNextTrack);
            menu.Items.Add(addQueueTrack);
            menu.Items.Add(exportTrack);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };
        // "Go to now playing" reveal: once the list realizes, bring the target
        // track's row into the outer scroller and flash it. selectedRelease was
        // chosen to contain it.
        if (scrollToTrackId is not null
            && selectedRelease.Tracks.FirstOrDefault(t => t.TrackId == scrollToTrackId) is { } revealTrack)
        {
            trackList.Loaded += (_, _) => FlashTrackRowWhenRealized(trackList, revealTrack, attemptsLeft: 8);
        }

        // The storage band, mirroring the dialog's: the selected release's storage
        // status, and while a transfer is in flight an indeterminate bar with the
        // verb plus a Cancel; otherwise its storage-action buttons.
        var storageBand = new StackPanel { Spacing = 4 };
        var storageRelease = selectedRelease;

        void RenderStorageBand()
        {
            storageBand.Children.Clear();
            var release = storageRelease;
            if (string.IsNullOrEmpty(release.ReleaseId))
            {
                return;
            }

            storageBand.Children.Add(new TextBlock
            {
                Text = DialogPrimitives.RestingStorageLabel(release.IsManaged, release.Pinned),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                FontSize = 12,
            });

            var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            var releaseId = release.ReleaseId;
            var token = _transfers.TokenFor(releaseId)
                ?? (release.TransferAction is { } transferAction
                    ? NativeBae.TransferActionToken(transferAction)
                    : null);
            if (token is not null && NativeBae.TransferActionKey(token) is { } verbKey)
            {
                row.Children.Add(new ProgressBar
                {
                    IsIndeterminate = true,
                    Width = 120,
                    VerticalAlignment = VerticalAlignment.Center,
                });
                row.Children.Add(new TextBlock
                {
                    Text = Loc.Core(verbKey),
                    VerticalAlignment = VerticalAlignment.Center,
                });
                var cancel = new Button { Content = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    var (cancelCurrent, error) = await _session.RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (cancelCurrent && error is not null)
                    {
                        _setStatus(error);
                    }
                };
                row.Children.Add(cancel);
            }
            else
            {
                foreach (var action in release.StorageActions)
                {
                    var act = action;
                    var button = new Button { Content = DialogPrimitives.StorageActionLabel(act) };
                    button.Click += async (_, _) =>
                    {
                        var error = await _storage.RunStorageActionForReleases(
                            act,
                            new List<string> { releaseId },
                            () => DialogPrimitives.PickUnmanageFolder(_windowHandle()));
                        if (error is not null)
                        {
                            _setStatus(error);
                        }
                    };
                    row.Children.Add(button);
                }
            }
            storageBand.Children.Add(row);
        }

        // Re-fetch the selected release's storage fields and re-render the band,
        // skipping when the release changed under the await.
        async System.Threading.Tasks.Task RefreshStorageBand()
        {
            var releaseId = selectedRelease.ReleaseId;
            if (string.IsNullOrEmpty(releaseId))
            {
                RenderStorageBand();
                return;
            }
            var (current2, result2) = await _session.RunForCurrentHandle(
                handle => NativeBae.ReleaseStorage(handle, releaseId));
            if (!current2 || selectedRelease.ReleaseId != releaseId)
            {
                return;
            }
            if (result2.Release is { } fresh)
            {
                storageRelease = fresh;
            }
            RenderStorageBand();
        }

        RenderStorageBand();

        var totalDuration = new TextBlock { Opacity = 0.7 };
        void RenderTotalDuration()
        {
            totalDuration.Text = selectedRelease.TotalDurationLabel;
            totalDuration.Visibility = string.IsNullOrEmpty(totalDuration.Text)
                ? Visibility.Collapsed
                : Visibility.Visible;
        }
        RenderTotalDuration();

        // The large cover on the left, bound OneWay to the selected release's own
        // cover so it fills in as the async load lands and re-points when the
        // picker switches release. A release with no cover of its own falls back to
        // the album card's cover rather than showing blank art.
        var coverImage = new Image { Stretch = Stretch.UniformToFill };
        void RebindCover()
        {
            coverImage.ClearValue(Image.SourceProperty);
            coverImage.SetBinding(Image.SourceProperty, selectedRelease.HasOwnCover
                ? new Binding { Source = selectedRelease, Path = new PropertyPath(nameof(Release.Cover)), Mode = BindingMode.OneWay }
                : new Binding { Source = card, Path = new PropertyPath(nameof(Album.Cover)), Mode = BindingMode.OneWay });
        }
        RebindCover();

        var detailStack = new StackPanel { Spacing = 8 };
        detailStack.Children.Add(title);
        detailStack.Children.Add(artist);
        // Release picker, only when the album has more than one pressing.
        if (detail.Releases.Count > 1)
        {
            var releasePicker = new ComboBox
            {
                Header = Loc.Chrome("album.release_picker"),
                ItemsSource = detail.Releases,
                SelectedItem = selectedRelease,
            };
            releasePicker.SelectionChanged += (_, _) =>
            {
                if (releasePicker.SelectedItem is Release release)
                {
                    selectedRelease = release;
                    trackList.ItemsSource = selectedRelease.Tracks;
                    storageRelease = release;
                    RebindCover();
                    RenderStorageBand();
                    RenderTotalDuration();
                }
            };
            detailStack.Children.Add(releasePicker);
        }
        detailStack.Children.Add(actions);
        detailStack.Children.Add(storageBand);
        detailStack.Children.Add(trackList);
        detailStack.Children.Add(totalDuration);

        // A click on the cover opens the release's gallery (the dialog's secondary
        // button).
        var coverBorder = new Border
        {
            Width = 300,
            Height = 300,
            CornerRadius = new CornerRadius(14),
            VerticalAlignment = VerticalAlignment.Top,
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = coverImage,
        };
        coverBorder.Tapped += async (_, _) =>
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                await _releaseActions.ShowGallery(selectedRelease.ReleaseId);
            }
        };

        var contentGrid = new Grid { ColumnSpacing = 32 };
        contentGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        contentGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        Grid.SetColumn(coverBorder, 0);
        Grid.SetColumn(detailStack, 1);
        contentGrid.Children.Add(coverBorder);
        contentGrid.Children.Add(detailStack);

        var panelCard = new Border
        {
            CornerRadius = new CornerRadius(18),
            Padding = new Thickness(24),
            Margin = new Thickness(0, 8, 0, 8),
            Background = (Brush)Application.Current.Resources["LayerFillColorDefaultBrush"],
            BorderThickness = new Thickness(1),
            BorderBrush = (Brush)Application.Current.Resources["CardStrokeColorDefaultBrush"],
            Child = contentGrid,
        };

        // Close affordance, top-trailing, mirroring the macOS ✕ — clears the
        // selected album, which collapses this panel.
        var closeButton = new Button
        {
            Content = new FontIcon { Glyph = "\uE711", FontSize = 12 },
            Background = new SolidColorBrush(Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(8),
            CornerRadius = new CornerRadius(9),
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Thickness(0, 16, 16, 0),
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(closeButton, Loc.Chrome("action.close"));
        closeButton.Click += (_, _) => _onClose();

        var root = new Grid { HorizontalAlignment = HorizontalAlignment.Stretch };
        root.Children.Add(panelCard);
        root.Children.Add(closeButton);

        // Keep the storage band live while the panel is open: a transfer event
        // re-renders it, and the Release/Album invalidation core emits alongside
        // re-fetches its storage fields. Torn down in Dispose.
        _registrations.Add(_projections.Register(typeof(BridgeInvalidation.Release), () => _ = RefreshStorageBand()));
        _registrations.Add(_projections.Register(typeof(BridgeInvalidation.Album), () => _ = RefreshStorageBand()));
        _onTransfersChanged = RenderStorageBand;
        _transfers.Changed += _onTransfersChanged;

        return root;
    }

    private void PlayRelease(Release release, bool shuffle)
    {
        if (!string.IsNullOrEmpty(release.ReleaseId))
        {
            _session.WithCurrentHandle(
                handle => NativeBae.PlayRelease(handle, release.ReleaseId, -1, shuffle));
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        if (_onTransfersChanged is not null)
        {
            _transfers.Changed -= _onTransfersChanged;
            _onTransfersChanged = null;
        }
        foreach (var registration in _registrations)
        {
            registration.Dispose();
        }
        _registrations.Clear();
    }
}
