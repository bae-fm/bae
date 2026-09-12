using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.VisualTree;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The window's menu bar: an in-window <see cref="Menu"/>, which is the Windows
/// and Linux convention (<c>NativeMenu</c> only reaches a menu bar on macOS).
/// It carries the same Library and Playback commands the macOS menu bar does.
///
/// Every item that has a shortcut shows it beside its label and registers it in
/// <see cref="WindowKeyBindings"/>, which the window installs, so the shortcut
/// works with the menu closed and there is one place a shortcut is declared.
///
/// The items that show state — the repeat mode, the two playback preferences,
/// whether the library can be shuffled — read their stores as the Playback menu
/// opens. That is the only moment they are visible, so what is on screen is
/// always what the app holds right then.
/// </summary>
internal sealed class MainMenuBar
{
    private readonly PlaybackCommands _playback;
    private readonly MenuCommand _shuffleLibrary;
    private readonly MenuItem _pauseBetweenSides;
    private readonly MenuItem _restoreOnLaunch;
    private readonly List<(BridgeRepeatMode Mode, MenuItem Item)> _repeatModes = new();
    private readonly List<KeyBinding> _keyBindings = new();

    public Menu View { get; }

    /// <summary>The shortcuts for this bar's items, for the window to install.
    /// A key binding on the window fires wherever focus is, which is what a menu
    /// shortcut means.</summary>
    public IReadOnlyList<KeyBinding> WindowKeyBindings => _keyBindings;

    public MainMenuBar(
        PlaybackCommands playback,
        Action openLibraries,
        Action openStorage,
        Action openSettings,
        Action closeLibrary)
    {
        _playback = playback;

        var library = new MenuItem { Header = Loc.Chrome("menu.library.title") };
        library.Items.Add(Item(Loc.Chrome("toolbar.libraries"), openLibraries));
        library.Items.Add(Item(Loc.Chrome("toolbar.storage"), openStorage));
        library.Items.Add(Item(Loc.Chrome("toolbar.settings"), openSettings));
        library.Items.Add(new Separator());
        library.Items.Add(Item(
            Loc.Chrome("toolbar.close_library"),
            closeLibrary,
            new KeyGesture(Key.W, KeyModifiers.Control | KeyModifiers.Shift)));

        var playbackMenu = new MenuItem { Header = Loc.Chrome("menu.playback.title") };
        playbackMenu.Items.Add(PlayPauseItem());
        playbackMenu.Items.Add(Item(
            Loc.Chrome("menu.playback.next_track"),
            _playback.NextTrack,
            new KeyGesture(Key.Right, KeyModifiers.Control | KeyModifiers.Alt)));
        playbackMenu.Items.Add(Item(
            Loc.Chrome("menu.playback.previous_track"),
            _playback.PreviousTrack,
            new KeyGesture(Key.Left, KeyModifiers.Control | KeyModifiers.Alt)));
        playbackMenu.Items.Add(Item(
            Loc.Chrome("menu.playback.mute"),
            _playback.ToggleMute,
            new KeyGesture(Key.M, KeyModifiers.Control | KeyModifiers.Alt)));
        playbackMenu.Items.Add(new Separator());
        playbackMenu.Items.Add(Item(
            Loc.Chrome("menu.playback.cycle_repeat"),
            _playback.CycleRepeatMode,
            new KeyGesture(Key.R, KeyModifiers.Control)));
        playbackMenu.Items.Add(RepeatSubmenu());
        playbackMenu.Items.Add(new Separator());

        // The Playback settings pane's two preferences, reachable without opening
        // settings. Each writes the same place the pane writes.
        _pauseBetweenSides = CheckableItem(
            Loc.Chrome("settings.playback.pause_between_sides"), _playback.TogglePauseBetweenSides);
        _restoreOnLaunch = CheckableItem(
            Loc.Chrome("settings.playback.restore_on_launch"), _playback.ToggleRestoreOnLaunch);
        playbackMenu.Items.Add(_pauseBetweenSides);
        playbackMenu.Items.Add(_restoreOnLaunch);
        playbackMenu.Items.Add(new Separator());

        var shuffle = Item(
            Loc.Chrome("ShuffleLibraryMenuItem.Text"),
            _playback.ShuffleLibrary,
            canRun: () => _playback.CanShuffleLibrary);
        _shuffleLibrary = (MenuCommand)shuffle.Command!;
        playbackMenu.Items.Add(shuffle);

        playbackMenu.SubmenuOpened += (_, _) => SyncPlaybackState();

        View = new Menu();
        View.Items.Add(library);
        View.Items.Add(playbackMenu);
    }

    // Re-read every item that shows state, as the Playback menu opens.
    private void SyncPlaybackState()
    {
        var mode = _playback.RepeatMode;
        foreach (var (candidate, item) in _repeatModes)
        {
            item.IsChecked = candidate == mode;
        }
        _pauseBetweenSides.IsChecked = _playback.PauseBetweenSides;
        _restoreOnLaunch.IsChecked = _playback.RestoreOnLaunch;
        _shuffleLibrary.RaiseCanExecuteChanged();
    }

    private MenuItem PlayPauseItem()
    {
        var gesture = new KeyGesture(Key.Space);
        var item = new MenuItem
        {
            Header = Loc.Chrome("menu.playback.play_pause"),
            Command = new MenuCommand(_playback.PlayPause),
            InputGesture = gesture,
        };
        // Space plays and pauses, except while a text field has focus: the space
        // being typed belongs to the field. The binding declines rather than the
        // window swallowing the key, which leaves the keystroke on its way there.
        _keyBindings.Add(new KeyBinding
        {
            Gesture = gesture,
            Command = new MenuCommand(_playback.PlayPause, () => !FocusIsInTextInput()),
        });
        return item;
    }

    private MenuItem RepeatSubmenu()
    {
        var repeat = new MenuItem { Header = Loc.Chrome("menu.playback.repeat") };
        // Listed in the order core steps through (Off → Context → Track), each
        // item setting its mode absolutely; the cycling command above walks the
        // same ring one step at a time.
        AddRepeatMode(repeat, BridgeRepeatMode.Off, "menu.playback.repeat_off");
        AddRepeatMode(repeat, BridgeRepeatMode.Context, "menu.playback.repeat_all");
        AddRepeatMode(repeat, BridgeRepeatMode.Track, "menu.playback.repeat_one");
        return repeat;
    }

    private void AddRepeatMode(MenuItem repeat, BridgeRepeatMode mode, string key)
    {
        var item = CheckableItem(Loc.Chrome(key), () => _playback.SetRepeatMode(mode));
        _repeatModes.Add((mode, item));
        repeat.Items.Add(item);
    }

    private MenuItem Item(string header, Action run, KeyGesture? gesture = null, Func<bool>? canRun = null)
    {
        var command = new MenuCommand(run, canRun);
        var item = new MenuItem { Header = header, Command = command, InputGesture = gesture };
        if (gesture is not null)
        {
            _keyBindings.Add(new KeyBinding { Gesture = gesture, Command = command });
        }
        return item;
    }

    // An item that carries a checkmark while what it names is on. The check is
    // written from the state on every open, never flipped by the click, so it
    // shows what the app holds rather than what the last press asked for.
    private static MenuItem CheckableItem(string header, Action run) => new()
    {
        Header = header,
        Command = new MenuCommand(run),
        ToggleType = MenuItemToggleType.CheckBox,
    };

    private bool FocusIsInTextInput() =>
        TopLevel.GetTopLevel(View)?.FocusManager?.GetFocusedElement() is Visual focused &&
        focused.FindAncestorOfType<TextBox>(includeSelf: true) is not null;
}
