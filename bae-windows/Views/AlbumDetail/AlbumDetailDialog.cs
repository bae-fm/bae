using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using uniffi.bae_bridge;
using Windows.System;

namespace Bae.Windows;

// The album-detail dialog: the album's releases (with a picker when there's more
// than one), the track list, and the per-release actions (play / queue / edit /
// re-identify / gallery / change cover / export / delete). Reused by the album
// grid, the composer/work panes, the "go to now playing" jump, and the import
// confirmation's "view in library" banner. Per-release actions each replace this
// dialog, so it closes and calls into the release-action dialogs after ShowAsync
// returns.
internal sealed class AlbumDetailDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly Action<string> _setStatus;
    private readonly ReleaseActionDialogs _releaseActions;
    private readonly StorageStore _storage;
    private readonly TransferProgressStore _transfers;
    private readonly ProjectionRegistry _projections;

    public AlbumDetailDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        Action<string> setStatus,
        ReleaseActionDialogs releaseActions,
        StorageStore storage,
        TransferProgressStore transfers,
        ProjectionRegistry projections)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _setStatus = setStatus;
        _releaseActions = releaseActions;
        _storage = storage;
        _transfers = transfers;
        _projections = projections;
    }

    public async System.Threading.Tasks.Task Show(
        string albumId,
        string? scrollToTrackId = null,
        string? initialReleaseId = null)
    {
        var (current, response) = await _session.RunForCurrentHandle(
            handle => NativeBae.GetAlbumDetail(handle, albumId));
        if (!current)
        {
            return;
        }
        if (response.Error is not null || response.Detail is null)
        {
            _setStatus(response.Error ?? Loc.Chrome("album.open_failed"));
            return;
        }

        var detail = response.Detail;

        if (detail.Releases.Count == 0)
        {
            _setStatus(Loc.Chrome("album.open_failed"));
            return;
        }

        // The release the dialog acts on: the user's primary, or the first. The
        // picker (added below when there's more than one) reassigns it, and every
        // per-release action and the track list read it, so switching release
        // retargets play / queue / edit / gallery / delete to that release.
        // When revealing a now-playing track, start on the release that actually
        // contains it — which may not be the primary.
        Release? trackRelease = scrollToTrackId is null
            ? null
            : detail.Releases.FirstOrDefault(r => r.Tracks.Any(t => t.TrackId == scrollToTrackId));
        Release selectedRelease = trackRelease
            ?? detail.Releases.FirstOrDefault(r => r.ReleaseId == initialReleaseId)
            ?? detail.Releases.FirstOrDefault(r => r.ReleaseId == detail.PrimaryReleaseId)
            ?? detail.Releases[0];

        var header = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
        };
        header.Children.Add(new TextBlock
        {
            Text = detail.Artist,
            VerticalAlignment = VerticalAlignment.Center,
        });
        var shuffleButton = new Button { Content = Loc.Chrome("album.shuffle") };
        var editButton = new Button { Content = Loc.Chrome("album.edit.label") };
        var reidentifyButton = new Button { Content = Loc.Chrome("album.reidentify.label") };
        header.Children.Add(shuffleButton);
        header.Children.Add(editButton);
        header.Children.Add(reidentifyButton);

        // Overflow menu: queueing + delete (release-level, so they need no track ids).
        var deleteRequested = false;
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
        // Set the selected release as the album's primary (canonical) one — only
        // meaningful when the album has more than one release.
        var setPrimaryItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.set_primary") };
        setPrimaryItem.Click += async (_, _) =>
        {
            var (setCurrent, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.SetPrimaryRelease(
                    handle,
                    detail.Id,
                    selectedRelease.ReleaseId));
            if (!setCurrent)
            {
                return;
            }
            _setStatus(error ?? Loc.Chrome("menu.set_primary_done"));
        };
        var changeCoverItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.change_cover") };
        var exportReleaseItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.export") };
        var saveAsReleaseItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.save_as") };
        var deleteItem = new MenuFlyoutItem { Text = Loc.Chrome("menu.delete") };
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
        header.Children.Add(moreButton);

        // Export and storage-action failures surface here, inside the dialog: the
        // window-level banner is occluded by this modal album-detail dialog, so a
        // banner error would be invisible until the dialog is dismissed.
        var statusLine = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // Track list: click a row to play the release from that track; right-tap
        // for per-track queueing. The play index is the track's position in the
        // release's track list, which is what PlayRelease expects.
        var trackList = new ListView
        {
            ItemsSource = selectedRelease.Tracks,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            // Bounded so ItemsStackPanel actually virtualizes (unbounded height
            // inside the ContentDialog's StackPanel realizes every row) and so
            // ScrollIntoView/ContainerFromItem below have a real viewport to
            // scroll within. Matches ImportDialog's list.
            MaxHeight = 320,
        };
        // "Go to now playing" reveal: once the list is realized, scroll the target
        // track into view and flash it. selectedRelease was chosen to contain it.
        if (scrollToTrackId is not null
            && selectedRelease.Tracks.FirstOrDefault(t => t.TrackId == scrollToTrackId) is { } revealTrack)
        {
            trackList.Loaded += (_, _) =>
            {
                trackList.ScrollIntoView(revealTrack);
                FlashTrackRowWhenRealized(trackList, revealTrack, attemptsLeft: 8);
            };
        }
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
            if (!queueCurrent)
            {
                return;
            }
            if (error is not null)
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
            exportTrack.Click += async (_, _) => await ExportTrack(track, statusLine);
            menu.Items.Add(play);
            menu.Items.Add(playNextTrack);
            menu.Items.Add(addQueueTrack);
            menu.Items.Add(exportTrack);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        // The storage band, mirroring macOS's StorageStatusBand: the selected
        // release's storage status, and while a transfer is in flight an
        // indeterminate bar with the verb plus a Cancel; otherwise its storage
        // action buttons. `storageRelease` carries the live storage fields the
        // band renders — seeded from the picked release, refreshed on the Release/
        // Album invalidations and transfer events core emits together.
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
                    statusLine.Visibility = Visibility.Collapsed;
                    var (cancelCurrent, error) = await _session.RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!cancelCurrent)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        statusLine.Text = error;
                        statusLine.Visibility = Visibility.Visible;
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
                        statusLine.Visibility = Visibility.Collapsed;
                        var error = await _storage.RunStorageActionForReleases(
                            act,
                            new List<string> { releaseId },
                            () => DialogPrimitives.PickUnmanageFolder(_windowHandle()));
                        if (error is not null)
                        {
                            statusLine.Text = error;
                            statusLine.Visibility = Visibility.Visible;
                        }
                    };
                    row.Children.Add(button);
                }
            }
            storageBand.Children.Add(row);
        }

        // Re-fetch the selected release's storage fields and re-render the band.
        // Skips when the release changed under the await (a newer refresh handles
        // the new one).
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

        // Total playing time, under the track list. Each pressing has its own
        // length, so picking a different release re-renders it.
        var totalDuration = new TextBlock
        {
            Opacity = 0.7,
            Text = selectedRelease.TotalDurationLabel,
        };
        void RenderTotalDuration()
        {
            totalDuration.Text = selectedRelease.TotalDurationLabel;
            totalDuration.Visibility = string.IsNullOrEmpty(totalDuration.Text)
                ? Visibility.Collapsed
                : Visibility.Visible;
        }
        RenderTotalDuration();

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(header);
        // Release picker, only when the album has more than one pressing. Choosing
        // a release swaps the track list and retargets every per-release action.
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
                    RenderStorageBand();
                    RenderTotalDuration();
                }
            };
            content.Children.Add(releasePicker);
        }
        content.Children.Add(storageBand);
        content.Children.Add(trackList);
        content.Children.Add(totalDuration);
        content.Children.Add(statusLine);

        var dialog = new ContentDialog
        {
            Title = detail.Title,
            Content = content,
            PrimaryButtonText = Loc.Chrome("action.play"),
            SecondaryButtonText = Loc.Chrome("album.gallery"),
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };

        // Edit and re-identify each replace the detail dialog: close it, then open
        // the chosen sheet on the primary release. ShowAsync resolves with None
        // when Hide() is called.
        var editRequested = false;
        var reidentifyRequested = false;
        var shuffleRequested = false;
        editButton.Click += (_, _) =>
        {
            editRequested = true;
            dialog.Hide();
        };
        reidentifyButton.Click += (_, _) =>
        {
            reidentifyRequested = true;
            dialog.Hide();
        };
        shuffleButton.Click += (_, _) =>
        {
            shuffleRequested = true;
            dialog.Hide();
        };
        // Changing the cover opens its own gallery dialog; a nested ContentDialog
        // can't open over this one, so close it first and open the gallery after
        // ShowAsync returns (the gallery/edit/re-identify pattern).
        var changeCoverRequested = false;
        var exportReleaseRequested = false;
        var saveReleaseAsRequested = false;
        changeCoverItem.Click += (_, _) =>
        {
            changeCoverRequested = true;
            dialog.Hide();
        };
        exportReleaseItem.Click += (_, _) =>
        {
            exportReleaseRequested = true;
            dialog.Hide();
        };
        saveAsReleaseItem.Click += (_, _) =>
        {
            saveReleaseAsRequested = true;
            dialog.Hide();
        };
        deleteItem.Click += (_, _) =>
        {
            deleteRequested = true;
            dialog.Hide();
        };

        // Keep the storage band live while the dialog is open: a transfer event
        // re-renders it (overlay-driven), and the Release/Album invalidation core
        // emits alongside re-fetches its storage fields. Disposed once ShowAsync
        // returns, the storage-dialog pattern.
        var registrations = new List<IDisposable>
        {
            _projections.Register(typeof(BridgeInvalidation.Release), () => _ = RefreshStorageBand()),
            _projections.Register(typeof(BridgeInvalidation.Album), () => _ = RefreshStorageBand()),
        };
        void OnTransfersChanged() => RenderStorageBand();
        _transfers.Changed += OnTransfersChanged;

        ContentDialogResult result;
        try
        {
            result = await dialog.ShowAsync();
        }
        finally
        {
            _transfers.Changed -= OnTransfersChanged;
            foreach (var registration in registrations)
            {
                registration.Dispose();
            }
        }

        if (editRequested)
        {
            await _releaseActions.ShowEditMetadata(selectedRelease.ReleaseId);
        }
        else if (reidentifyRequested)
        {
            await _releaseActions.ShowReidentify(selectedRelease.ReleaseId, detail.Artist, detail.Title);
        }
        else if (shuffleRequested)
        {
            if (!string.IsNullOrEmpty(selectedRelease.ReleaseId))
            {
                _session.WithCurrentHandle(
                    handle => NativeBae.PlayRelease(handle, selectedRelease.ReleaseId, -1, true));
            }
        }
        else if (changeCoverRequested)
        {
            await _releaseActions.ShowChangeCover(detail.Id, selectedRelease.ReleaseId);
        }
        else if (exportReleaseRequested && !string.IsNullOrEmpty(selectedRelease.ReleaseId))
        {
            await _releaseActions.ShowExportRelease(selectedRelease.ReleaseId);
        }
        else if (saveReleaseAsRequested && !string.IsNullOrEmpty(selectedRelease.ReleaseId))
        {
            await _releaseActions.ShowSaveReleaseAs(selectedRelease.ReleaseId);
        }
        else if (deleteRequested)
        {
            await _releaseActions.ConfirmDeleteRelease(selectedRelease.ReleaseId);
        }
        else if (result == ContentDialogResult.Primary && !string.IsNullOrEmpty(selectedRelease.ReleaseId))
        {
            _session.WithCurrentHandle(
                handle => NativeBae.PlayRelease(handle, selectedRelease.ReleaseId, -1, false));
        }
        else if (result == ContentDialogResult.Secondary)
        {
            await _releaseActions.ShowGallery(selectedRelease.ReleaseId);
        }
    }

    // The per-track "Save As…": choose a track-applicable preset, seed the
    // filename from the configured template, then save to the picked path. Errors
    // surface in the album-detail status line, which the modal doesn't occlude
    // here (the pickers are OS dialogs).
    private async System.Threading.Tasks.Task ExportTrack(Track track, TextBlock statusLine)
    {
        statusLine.Visibility = Visibility.Collapsed;
        var picker = new global::Windows.Storage.Pickers.FileSavePicker();
        WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
        var (settingsCurrent, settings) = await _session.RunForCurrentHandle(NativeBae.GetSettings);
        if (!settingsCurrent)
        {
            return;
        }
        var trackPresets = settings.SavePresets
            .Where(preset => preset.AppliesToTrack)
            .ToList();
        // Config validation guarantees at least one track-applicable preset and a
        // valid default; guard anyway rather than crash if that's ever violated.
        if (trackPresets.Count == 0)
        {
            statusLine.Text = Loc.Chrome("track.export.prepare_failed");
            statusLine.Visibility = Visibility.Visible;
            return;
        }
        var formatPicker = new ComboBox
        {
            Header = Loc.Chrome("settings.formats.default_track_format"),
            MinWidth = 260,
        };
        var defaultIndex = 0;
        for (var index = 0; index < trackPresets.Count; index++)
        {
            if (trackPresets[index].Id == settings.DefaultTrackSavePreset)
            {
                defaultIndex = index;
            }
            formatPicker.Items.Add(new ComboBoxItem
            {
                Content = trackPresets[index].TrackPickerLabel,
            });
        }
        formatPicker.SelectedIndex = defaultIndex;
        var formatDialog = new ContentDialog
        {
            Title = Loc.Chrome("save.title"),
            Content = formatPicker,
            PrimaryButtonText = Loc.Chrome("action.save"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };
        var formatResult = await formatDialog.ShowAsync();
        if (formatResult != ContentDialogResult.Primary)
        {
            return;
        }
        if (formatPicker.SelectedIndex < 0)
        {
            statusLine.Text = Loc.Chrome("track.export.prepare_failed");
            statusLine.Visibility = Visibility.Visible;
            return;
        }
        var selectedPreset = trackPresets[formatPicker.SelectedIndex];
        picker.FileTypeChoices.Add(
            selectedPreset.TrackPickerLabel,
            new List<string> { selectedPreset.FileExtension });
        // Seed the suggested name from the chosen preset's filename pattern,
        // which the core renders and sanitizes from this track's metadata. The
        // format dialog runs before the picker, so one call suffices. A null
        // return — or a throw — means that render failed (the core logged the
        // cause); surface it and abort rather than saving under a guessed name.
        string? stem;
        try
        {
            var (nameCurrent, suggestedName) = await _session.RunForCurrentHandle(
                handle => NativeBae.SaveTrackSuggestedName(
                    handle, track.TrackId, selectedPreset.Id));
            if (!nameCurrent)
            {
                return;
            }
            stem = suggestedName;
        }
        catch (Exception ex)
        {
            BaeDiagnostics.Logger.Error("save suggested-name lookup threw", ex);
            stem = null;
        }
        if (stem is null)
        {
            statusLine.Text = Loc.Chrome("track.export.prepare_failed");
            statusLine.Visibility = Visibility.Visible;
            return;
        }
        picker.SuggestedFileName = stem;
        var file = await picker.PickSaveFileAsync();
        if (file is null)
        {
            return;
        }

        var path = file.Path;
        var (saveCurrent, error) = await _session.RunForCurrentHandle(
            handle => NativeBae.SaveTrack(handle, track.TrackId, path, selectedPreset.Id));
        if (!saveCurrent)
        {
            return;
        }
        if (error is not null)
        {
            statusLine.Text = error;
            statusLine.Visibility = Visibility.Visible;
        }
    }

    // ScrollIntoView realizes the target row's container only after a later layout
    // pass, so poll for it across a few UI ticks before flashing. Without this the
    // flash silently no-ops for any track below the initial viewport — the common
    // case for "go to now playing". Gives up after a bounded number of attempts.
    private static void FlashTrackRowWhenRealized(ListView list, Track track, int attemptsLeft)
    {
        if (list.ContainerFromItem(track) is ListViewItem row)
        {
            FlashRow(row);
            return;
        }

        if (attemptsLeft <= 0)
        {
            return;
        }

        list.DispatcherQueue.TryEnqueue(() => FlashTrackRowWhenRealized(list, track, attemptsLeft - 1));
    }

    // Tint a row with the system accent and fade it out over three seconds — the
    // "go to now playing" flash, mirroring macOS.
    private static void FlashRow(ListViewItem row)
    {
        var accent = new global::Windows.UI.ViewManagement.UISettings()
            .GetColorValue(global::Windows.UI.ViewManagement.UIColorType.Accent);
        var brush = new SolidColorBrush(accent) { Opacity = 0.35 };
        row.Background = brush;

        var fade = new DoubleAnimation
        {
            To = 0,
            Duration = new Duration(TimeSpan.FromSeconds(3)),
            EnableDependentAnimation = true,
        };
        Storyboard.SetTarget(fade, brush);
        Storyboard.SetTargetProperty(fade, "Opacity");
        var storyboard = new Storyboard();
        storyboard.Children.Add(fade);
        storyboard.Begin();
    }
}
