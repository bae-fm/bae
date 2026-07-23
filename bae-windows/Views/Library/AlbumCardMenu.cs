using System;
using System.Threading.Tasks;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// The album card's context menu: play, queue next / at the end, or pin for
// offline — the release-level actions reachable without opening the album.
// targetCount is how many albums the click resolved to (the whole
// multi-selection for a member card, else just the clicked one, computed by
// AlbumGridSelectionModel.OrderedTargets); labels carry the count only when
// more than one album is targeted. Opening the menu never mutates the
// selection.
internal static class AlbumCardMenu
{
    internal static MenuFlyout Build(
        int targetCount,
        Func<Task> onPlay,
        Func<Task> onPlayNext,
        Func<Task> onAddToQueue,
        Func<Task> onPin)
    {
        var menu = new MenuFlyout();
        menu.Items.Add(Item(Label("menu.play", "menu.play_count", targetCount), onPlay));
        menu.Items.Add(Item(Label("menu.play_next", "menu.play_next_count", targetCount), onPlayNext));
        menu.Items.Add(Item(Label("menu.add_to_queue", "menu.add_to_queue_count", targetCount), onAddToQueue));
        menu.Items.Add(Item(Label("menu.pin", "menu.pin_count", targetCount), onPin));
        return menu;
    }

    private static string Label(string singularKey, string countKey, int targetCount) =>
        targetCount > 1 ? Loc.Chrome(countKey, "count", targetCount) : Loc.Chrome(singularKey);

    private static MenuFlyoutItem Item(string text, Func<Task> onClick)
    {
        var item = new MenuFlyoutItem { Text = text };
        item.Click += async (_, _) => await onClick();
        return item;
    }
}
