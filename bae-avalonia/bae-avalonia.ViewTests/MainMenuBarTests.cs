using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The window's menu bar: which menus it has, what the Playback menu lists and
/// in what order, the shortcuts beside the labels and on the window, the state
/// each item shows when the menu opens, and that pressing an item runs its
/// command once.
/// </summary>
public sealed class MainMenuBarTests
{
    [AvaloniaFact]
    public void TheBarHasALibraryMenuAndAPlaybackMenu()
    {
        var (bar, _) = Build();

        Assert.Equal(
            new object?[] { Loc.Chrome("menu.library.title"), Loc.Chrome("menu.playback.title") },
            bar.View.Items.Cast<MenuItem>().Select(item => item.Header).ToArray());
    }

    [AvaloniaFact]
    public void TheLibraryMenuListsItsCommandsWithCloseLast()
    {
        var (bar, _) = Build();

        Assert.Equal(
            new object?[]
            {
                Loc.Chrome("toolbar.libraries"),
                Loc.Chrome("toolbar.storage"),
                Loc.Chrome("toolbar.settings"),
                null,
                Loc.Chrome("toolbar.close_library"),
            },
            Headers(LibraryMenu(bar)));
        Assert.Equal(
            new KeyGesture(Key.W, KeyModifiers.Control | KeyModifiers.Shift).ToString(),
            Items(LibraryMenu(bar)).Last().InputGesture?.ToString());
    }

    [AvaloniaFact]
    public void ThePlaybackMenuListsItsItemsInOrder()
    {
        var (bar, _) = Build();

        Assert.Equal(
            new object?[]
            {
                Loc.Chrome("menu.playback.play_pause"),
                Loc.Chrome("menu.playback.next_track"),
                Loc.Chrome("menu.playback.previous_track"),
                Loc.Chrome("menu.playback.mute"),
                null,
                Loc.Chrome("menu.playback.cycle_repeat"),
                Loc.Chrome("menu.playback.repeat"),
                null,
                Loc.Chrome("settings.playback.pause_between_sides"),
                Loc.Chrome("settings.playback.restore_on_launch"),
                null,
                Loc.Chrome("ShuffleLibraryMenuItem.Text"),
            },
            Headers(PlaybackMenu(bar)));
    }

    [AvaloniaFact]
    public void EachShortcutShowsBesideItsLabelAndIsAWindowKeyBinding()
    {
        var (bar, _) = Build();
        var expected = new Dictionary<string, KeyGesture>
        {
            [Loc.Chrome("menu.playback.play_pause")] = new(Key.Space),
            [Loc.Chrome("menu.playback.next_track")] = new(Key.Right, KeyModifiers.Control | KeyModifiers.Alt),
            [Loc.Chrome("menu.playback.previous_track")] = new(Key.Left, KeyModifiers.Control | KeyModifiers.Alt),
            [Loc.Chrome("menu.playback.mute")] = new(Key.M, KeyModifiers.Control | KeyModifiers.Alt),
            [Loc.Chrome("menu.playback.cycle_repeat")] = new(Key.R, KeyModifiers.Control),
        };

        foreach (var (header, gesture) in expected)
        {
            var item = Items(PlaybackMenu(bar)).Single(candidate => Equals(candidate.Header, header));
            Assert.Equal(gesture.ToString(), item.InputGesture?.ToString());
        }

        var bound = bar.WindowKeyBindings.Select(binding => binding.Gesture?.ToString()).ToList();
        foreach (var gesture in expected.Values)
        {
            Assert.Contains(gesture.ToString(), bound);
        }
        Assert.Contains(
            new KeyGesture(Key.W, KeyModifiers.Control | KeyModifiers.Shift).ToString(), bound);
    }

    [AvaloniaFact]
    public void TheRepeatSubmenuChecksTheModeTheStoreHolds()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyRepeat(BridgeRepeatMode.Context);
        Open(bar);

        Assert.Equal(
            new[] { false, true, false },
            RepeatItems(bar).Select(item => item.IsChecked).ToArray());

        fakes.Store.ApplyRepeat(BridgeRepeatMode.Track);
        Open(bar);

        Assert.Equal(
            new[] { false, false, true },
            RepeatItems(bar).Select(item => item.IsChecked).ToArray());
    }

    [AvaloniaFact]
    public void ThePreferenceItemsCheckWhatIsStored()
    {
        var (bar, fakes) = Build(pauseBetweenSides: true, restoreOnLaunch: false);
        Open(bar);

        Assert.True(PlaybackItem(bar, "settings.playback.pause_between_sides").IsChecked);
        Assert.False(PlaybackItem(bar, "settings.playback.restore_on_launch").IsChecked);

        fakes.RestoreOnLaunch = true;
        Open(bar);

        Assert.True(PlaybackItem(bar, "settings.playback.restore_on_launch").IsChecked);
    }

    [AvaloniaFact]
    public void ShuffleLibraryIsUnavailableUntilTheLibraryHasAnAlbum()
    {
        var (bar, fakes) = Build(albumCount: 0);
        Open(bar);

        Assert.False(PlaybackItem(bar, "ShuffleLibraryMenuItem.Text").IsEffectivelyEnabled);

        fakes.AlbumCount = 3;
        Open(bar);

        Assert.True(PlaybackItem(bar, "ShuffleLibraryMenuItem.Text").IsEffectivelyEnabled);
    }

    [AvaloniaFact]
    public void PressingEachItemRunsItsCommandOnce()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);

        foreach (var item in Items(LibraryMenu(bar)).Concat(Items(PlaybackMenu(bar))).Concat(RepeatItems(bar)))
        {
            // The Repeat item only opens its own submenu, so it has no command.
            if (item.Command is not null)
            {
                item.RaiseEvent(new RoutedEventArgs(MenuItem.ClickEvent));
            }
        }

        Assert.Equal(
            new[]
            {
                "libraries", "storage", "settings", "close library",
                "pause", "next track", "previous track", "mute", "set repeat Context",
                "pause between sides", "restore on launch", "shuffle library",
                "set repeat Off", "set repeat Context", "set repeat Track",
            },
            fakes.Ran);
    }

    /// <summary>Space plays and pauses — but a space typed into a text field
    /// belongs to the field, so the binding declines and the keystroke is left
    /// for it.</summary>
    [AvaloniaFact]
    public void SpaceRunsPlayPauseOnlyWhileNoTextFieldHasFocus()
    {
        var (bar, fakes) = Build();
        fakes.Store.ApplyPlaying("album", "track", "Title", "Artist", null);
        var box = new TextBox();
        var window = Host(bar, box);

        window.KeyPress(Key.Space, RawInputModifiers.None, PhysicalKey.Space, " ");
        Assert.Equal(new[] { "pause" }, fakes.Ran);

        box.Focus();
        Dispatcher.UIThread.RunJobs();
        window.KeyPress(Key.Space, RawInputModifiers.None, PhysicalKey.Space, " ");

        Assert.Equal(new[] { "pause" }, fakes.Ran);
    }

    // ── Fixture ──────────────────────────────────────────────────────────────

    /// <summary>Every action the bar can run, recorded in the order it ran, over
    /// a playback service that reports each command instead of reaching a
    /// library.</summary>
    private sealed class Fakes
    {
        public readonly List<string> Ran = new();
        public PlaybackStore Store = null!;
        public int AlbumCount = 1;
        public bool RestoreOnLaunch;
    }

    private static (MainMenuBar Bar, Fakes Fakes) Build(
        int albumCount = 1,
        bool pauseBetweenSides = false,
        bool restoreOnLaunch = false)
    {
        Dispatcher.UIThread.VerifyAccess();

        var fakes = new Fakes { AlbumCount = albumCount, RestoreOnLaunch = restoreOnLaunch };
        fakes.Store = new PlaybackStore(new QueueService(), _ => { });
        var settings = new SettingsStore(new SettingsService
        {
            GetSettings = () => (true, new Settings { PauseBetweenSides = pauseBetweenSides }),
        });
        settings.Reload();

        bool Ran(string command)
        {
            fakes.Ran.Add(command);
            return true;
        }

        var playback = new PlaybackService
        {
            Pause = () => Ran("pause"),
            Resume = () => Ran("resume"),
            NextTrack = () => Ran("next track"),
            PreviousTrack = () => Ran("previous track"),
            SetMuted = _ => Ran("mute"),
            // Which mode follows which is core's; the fake answers with one the
            // menu never sets directly, so a cycle is told apart from a pick.
            NextRepeatMode = _ => BridgeRepeatMode.Context,
            SetRepeatMode = mode => Ran($"set repeat {mode}"),
            PlayLibraryShuffled = () => Ran("shuffle library"),
            SetPauseBetweenSides = _ =>
            {
                fakes.Ran.Add("pause between sides");
                return (true, null);
            },
        };

        var commands = new PlaybackCommands(
            playback,
            fakes.Store,
            settings,
            () => fakes.AlbumCount,
            () => fakes.RestoreOnLaunch,
            value =>
            {
                fakes.RestoreOnLaunch = value;
                fakes.Ran.Add("restore on launch");
            },
            _ => { });

        var bar = new MainMenuBar(
            commands,
            openLibraries: () => fakes.Ran.Add("libraries"),
            openStorage: () => fakes.Ran.Add("storage"),
            openSettings: () => fakes.Ran.Add("settings"),
            closeLibrary: () => fakes.Ran.Add("close library"));
        return (bar, fakes);
    }

    /// <summary>The bar in a shown window, which is what makes its items part of
    /// a visual tree and its key bindings reachable.</summary>
    private static Window Host(MainMenuBar bar, Control? below = null)
    {
        var layout = new DockPanel();
        DockPanel.SetDock(bar.View, Dock.Top);
        layout.Children.Add(bar.View);
        if (below is not null)
        {
            layout.Children.Add(below);
        }
        var window = new Window { Content = layout };
        foreach (var binding in bar.WindowKeyBindings)
        {
            window.KeyBindings.Add(binding);
        }
        window.Show();
        return window;
    }

    // Open the Playback menu, which is when its items read what they show, and
    // leave it open so the test reads them the way they are rendered. Closing
    // first lets a test open it again after changing what it holds.
    private static void Open(MainMenuBar bar)
    {
        if (bar.View.GetVisualRoot() is null)
        {
            Host(bar);
        }
        var menu = PlaybackMenu(bar);
        menu.Close();
        menu.Open();
        Dispatcher.UIThread.RunJobs();
    }

    private static MenuItem LibraryMenu(MainMenuBar bar) => (MenuItem)bar.View.Items[0]!;

    private static MenuItem PlaybackMenu(MainMenuBar bar) => (MenuItem)bar.View.Items[1]!;

    private static MenuItem PlaybackItem(MainMenuBar bar, string key) =>
        Items(PlaybackMenu(bar)).Single(item => Equals(item.Header, Loc.Chrome(key)));

    private static IEnumerable<MenuItem> RepeatItems(MainMenuBar bar) =>
        Items(PlaybackItem(bar, "menu.playback.repeat"));

    private static IEnumerable<MenuItem> Items(MenuItem menu) => menu.Items.OfType<MenuItem>();

    // A separator reads as a null header, so the order check covers where the
    // rules fall as well as what the items are.
    private static object?[] Headers(MenuItem menu) =>
        menu.Items.Select(item => item is MenuItem entry ? entry.Header : null).ToArray();
}
