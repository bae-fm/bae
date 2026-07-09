using System;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// The album card's context menu: play the album's canonical release, or queue
// it next / at the end — the same release-level actions the album-detail
// overflow menu offers, reachable without opening the album.
internal static class AlbumCardMenu
{
    internal static MenuFlyout Build(Action onPlay, Action onPlayNext, Action onAddToQueue)
    {
        var menu = new MenuFlyout();
        var play = new MenuFlyoutItem { Text = Loc.Chrome("menu.play") };
        play.Click += (_, _) => onPlay();
        var playNext = new MenuFlyoutItem { Text = Loc.Chrome("menu.play_next") };
        playNext.Click += (_, _) => onPlayNext();
        var addToQueue = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
        addToQueue.Click += (_, _) => onAddToQueue();
        menu.Items.Add(play);
        menu.Items.Add(playNext);
        menu.Items.Add(addToQueue);
        return menu;
    }
}
