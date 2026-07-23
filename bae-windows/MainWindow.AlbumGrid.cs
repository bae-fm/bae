using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics;
using Windows.Storage;
using Windows.System;

namespace Bae.Windows;

// MainWindow: album click/right-tap, the card menus, multi-select tint, and the
// storage/settings toolbar clicks. Split out of MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    // Dispatch by the modifiers held at click time: Ctrl toggles the clicked
    // album, Shift extends the range from the anchor, and a plain click toggles
    // the album's inline detail expansion (clearing the multi-selection).
    // Modifier clicks never open the expansion. Per-card (the grid ListView
    // virtualizes rows, not cards), so the album is the card's DataContext.
    private void OnAlbumCardTapped(object sender, TappedRoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null || (sender as FrameworkElement)?.DataContext is not Album album)
        {
            return;
        }

        if (IsModifierDown(VirtualKey.Control))
        {
            _albumSelection.Toggle(album.Id);
            SyncAlbumSelectionTint();
            return;
        }
        if (IsModifierDown(VirtualKey.Shift))
        {
            _albumSelection.ExtendRange(album.Id, AlbumPosition, AlbumIdAt);
            SyncAlbumSelectionTint();
            return;
        }

        ToggleAlbumExpansion(album);
    }

    // Right-click / long-press on an album card: the bulk-action menu for
    // whatever the card targets — the whole multi-selection (visible order)
    // for a member card, else just that card. Never mutates the selection.
    private void OnAlbumCardRightTapped(object sender, RightTappedRoutedEventArgs e)
    {
        if (CurrentHandleOrNull() == null || (sender as FrameworkElement)?.DataContext is not Album album)
        {
            return;
        }

        var targets = _albumSelection.OrderedTargets(album.Id, AlbumPosition);
        var menu = targets.Count == 1
            ? BuildSingleAlbumCardMenu(album)
            : BuildBulkAlbumCardMenu(targets);
        if (menu is null)
        {
            return;
        }

        e.Handled = true;
        var element = (FrameworkElement)sender;
        menu.ShowAt(element, new FlyoutShowOptions { Position = e.GetPosition(element) });
    }

    // The pre-existing single-album menu: release-based actions on the card's
    // primary release. Null when the album carries none (defensive — every
    // grid-loaded album has one; only a search-result album wouldn't).
    private MenuFlyout? BuildSingleAlbumCardMenu(Album album)
    {
        var releaseId = album.PrimaryReleaseId;
        if (string.IsNullOrEmpty(releaseId))
        {
            return null;
        }
        return AlbumCardMenu.Build(
            targetCount: 1,
            onPlay: () =>
            {
                WithCurrentHandle(handle => NativeBae.PlayRelease(handle, releaseId, -1, false));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPlayNext: () =>
            {
                WithCurrentHandle(handle => NativeBae.AddReleaseNext(handle, releaseId));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onAddToQueue: () =>
            {
                WithCurrentHandle(handle => NativeBae.AddReleaseToQueue(handle, releaseId));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPin: () => PinReleases(new[] { releaseId }));
    }

    // The bulk menu for a multi-selected card: batch actions over every
    // targeted album, in visible grid order.
    private MenuFlyout BuildBulkAlbumCardMenu(IReadOnlyList<string> targets)
    {
        var primaryReleaseIds = PrimaryReleaseIds(targets);
        return AlbumCardMenu.Build(
            targetCount: targets.Count,
            onPlay: () =>
            {
                WithCurrentHandle(handle => NativeBae.PlayReleases(handle, primaryReleaseIds));
                return System.Threading.Tasks.Task.CompletedTask;
            },
            onPlayNext: () => _queuePane.AddAlbumsToQueue(targets, addNext: true),
            onAddToQueue: () => _queuePane.AddAlbumsToQueue(targets, addNext: false),
            onPin: () => PinReleases(primaryReleaseIds));
    }

    // Enqueue releases to pin for offline, surfacing a failure through the
    // shell error banner. Runs off the UI thread: the pin enqueue awaits core's
    // async library-manager call.
    private async System.Threading.Tasks.Task PinReleases(IReadOnlyList<string> releaseIds)
    {
        var (current, error) = await _session.RunForCurrentHandle(handle => releaseIds.Count == 1
            ? NativeBae.PinRelease(handle, releaseIds[0])
            : NativeBae.PinReleases(handle, releaseIds));
        if (current && error is not null)
        {
            _shell.ShowBanner(InfoBarSeverity.Error, Loc.Chrome("error.title"), error);
        }
    }

    // The targeted albums' primary release ids, in target order, dropping any
    // target with none (a search-result album would have none; grid albums
    // always do).
    private List<string> PrimaryReleaseIds(IReadOnlyList<string> albumIds)
    {
        var albumsById = Albums.ToDictionary(album => album.Id);
        return albumIds
            .Select(id => albumsById.TryGetValue(id, out var album) ? album.PrimaryReleaseId : null)
            .Where(releaseId => !string.IsNullOrEmpty(releaseId))
            .Select(releaseId => releaseId!)
            .ToList();
    }

    // The clicked album's index in the loaded grid, or null if it isn't loaded —
    // the position delegate AlbumGridSelectionModel needs for range-extend and
    // ordered targets.
    private int? AlbumPosition(string id)
    {
        for (var i = 0; i < Albums.Count; i++)
        {
            if (Albums[i].Id == id)
            {
                return i;
            }
        }
        return null;
    }

    private string? AlbumIdAt(int index) =>
        index >= 0 && index < Albums.Count ? Albums[index].Id : null;

    // Sync every loaded album's tint from the selection model. Called after
    // every mutation; O(loaded count), which is at most the first page (500).
    private void SyncAlbumSelectionTint()
    {
        foreach (var album in Albums)
        {
            album.IsSelected = _albumSelection.Contains(album.Id);
        }
    }

    private static bool IsModifierDown(VirtualKey key) =>
        Microsoft.UI.Input.InputKeyboardSource.GetKeyStateForCurrentThread(key)
            .HasFlag(global::Windows.UI.Core.CoreVirtualKeyStates.Down);
}
