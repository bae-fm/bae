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

// MainWindow: the playback transport controls, keyboard accelerators, the
// global key handler, and volume. Split out of MainWindow.xaml.cs unchanged.
public sealed partial class MainWindow : Window
{
    private void OnPlayPause(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            DispatchPlayPause();
        }
    }

    // The play/pause circle grows slightly on hover and settles back on exit.
    private void OnPlayPausePointerEntered(object sender, PointerRoutedEventArgs e) => AnimatePlayScale(1.05);

    private void OnPlayPausePointerExited(object sender, PointerRoutedEventArgs e) => AnimatePlayScale(1.0);

    private void AnimatePlayScale(double target)
    {
        var storyboard = new Storyboard();
        foreach (var axis in new[] { "ScaleX", "ScaleY" })
        {
            var animation = new DoubleAnimation
            {
                To = target,
                Duration = new Duration(TimeSpan.FromMilliseconds(120)),
                EasingFunction = new QuadraticEase { EasingMode = EasingMode.EaseOut },
                EnableDependentAnimation = true,
            };
            Storyboard.SetTarget(animation, NpPlayScale);
            Storyboard.SetTargetProperty(animation, axis);
            storyboard.Children.Add(animation);
        }
        storyboard.Begin();
    }

    // A play/pause press names its target: pause what plays, resume what's
    // paused, nothing when stopped (there is no track to act on).
    private void DispatchPlayPause()
    {
        switch (_playback.PlayState)
        {
            case TransportPlayState.Playing:
                WithCurrentHandle(NativeBae.Pause);
                break;
            case TransportPlayState.Paused:
                WithCurrentHandle(NativeBae.Resume);
                break;
            case TransportPlayState.Stopped:
                break;
        }
    }

    private void OnNext(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.Next);
        }
    }

    private void OnPrevious(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(NativeBae.Previous);
        }
    }

    private void OnRepeat(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(handle => NativeBae.SetRepeatMode(handle, NativeBae.NextRepeatMode(_playback.RepeatMode)));
        }
    }

    // Give an icon-only button a fixed accessible name and matching tooltip from
    // a chrome catalog key.
    private static void SetIconButtonLabel(Button button, string key)
    {
        var label = Loc.Chrome(key);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, label);
        ToolTipService.SetToolTip(button, label);
    }

    // Shuffle names its target absolutely: the opposite of the playing context's
    // current shuffled flag. The button is disabled when there's no context.
    private void OnShuffle(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(handle => NativeBae.SetShuffle(handle, !(_playback.Context?.Shuffled ?? false)));
        }
    }

    private void OnMute(object sender, RoutedEventArgs e)
    {
        if (CurrentHandleOrNull() != null)
        {
            WithCurrentHandle(handle => NativeBae.SetMuted(handle, !_playback.IsMuted));
        }
    }

    // Ctrl+F focuses the search box from anywhere in the window.
    private void OnFocusSearchAccelerator(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        SearchBox.Focus(FocusState.Programmatic);
        args.Handled = true;
    }

    // Ctrl+L jumps to whatever's playing: open its album's detail and scroll the
    // track into view, flashing it. No-op when nothing is playing.
    private async void OnGoToNowPlaying(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        await OpenNowPlayingAlbum();
    }

    // Ctrl+1..9 switch to the Nth discovered library. No-op when the digit is
    // beyond the list or already the active library; open failures land on the
    // existing status text and unlock dialog, like any other switch.
    private async void OnLibrarySwitchAccelerator(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        var digit = (int)sender.Key - (int)VirtualKey.Number0;
        // Only openable libraries are switch targets: the shortcut opens without
        // asking, and a broken library cannot be opened.
        var libraries = LoadLibraries().Where(library => library.Error is null).ToList();
        var target = LibrarySwitchModel.TargetLibraryId(
            libraries.ConvertAll(library => (library.Id, library.IsActive)), digit);
        if (target is not null)
        {
            await SwitchLibrary(target);
        }
    }

    // Clicking the bar's album art or track title jumps to the playing album —
    // the pointer version of the go-to-now-playing accelerator.
    private async void OnNowPlayingInfoTapped(object sender, TappedRoutedEventArgs e)
    {
        e.Handled = true;
        await OpenNowPlayingAlbum();
    }

    // Open the playing track's album and scroll it into view. No-op when nothing
    // is playing.
    private async System.Threading.Tasks.Task OpenNowPlayingAlbum()
    {
        var albumId = _playback.NowPlayingAlbumId;
        if (CurrentHandleOrNull() == null || string.IsNullOrEmpty(albumId))
        {
            return;
        }

        await _albumDetail.Show(albumId, scrollToTrackId: _playback.NowPlayingTrackId);
    }

    // Space toggles play/pause from anywhere — except while typing in a text
    // field, where space must insert a space. Handled here, not as a button
    // accelerator, so a bare Space key never steals input from a text box.
    // Dialog/flyout text inputs are safe for free: they live in separate popups,
    // not under this root Grid, so their KeyDown never bubbles here. The focus
    // check only has to cover text inputs in the main tree — the search box and
    // the welcome chooser's restore-code box.
    private void OnGlobalKeyDown(object sender, KeyRoutedEventArgs e)
    {
        // Escape closes the queue pane when it's open. Dialogs capture their own
        // input layer, so their Escape never reaches this root handler. The
        // queue pane keeps priority: the album-grid selection only clears once
        // it isn't open.
        if (e.Key == VirtualKey.Escape && _queuePane.IsOpen)
        {
            _queuePane.Hide();
            e.Handled = true;
            return;
        }

        if (e.Key == VirtualKey.Escape && AlbumGrid.Visibility == Visibility.Visible && !_albumSelection.IsEmpty)
        {
            _albumSelection.Clear();
            SyncAlbumSelectionTint();
            e.Handled = true;
            return;
        }

        var focused = FocusManager.GetFocusedElement(Content.XamlRoot);
        var focusedTextInput = focused is TextBox || focused is AutoSuggestBox;

        // Ctrl+A selects every loaded album, guarded by the same focused-text-input
        // check as Space so Ctrl+A in the search box still selects its text.
        if (e.Key == VirtualKey.A && !focusedTextInput
            && AlbumGrid.Visibility == Visibility.Visible && IsModifierDown(VirtualKey.Control))
        {
            _albumSelection.SelectAll(Albums.Select(album => album.Id).ToList());
            SyncAlbumSelectionTint();
            e.Handled = true;
            return;
        }

        if (e.Key != VirtualKey.Space || focusedTextInput)
        {
            return;
        }

        if (CurrentHandleOrNull() != null)
        {
            DispatchPlayPause();
            e.Handled = true;
        }
    }

    private void OnVolumeChanged(object sender, Microsoft.UI.Xaml.Controls.Primitives.RangeBaseValueChangedEventArgs e)
    {
        _nowPlayingBar.HandleVolumeSliderChanged();
    }
}
