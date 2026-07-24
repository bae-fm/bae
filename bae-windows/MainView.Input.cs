using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Windows.System;

namespace Bae.Windows;

// MainView: the view's root input surface — the keyboard accelerators on the
// root grid and the global KeyDown — routed to whichever feature view owns each
// action. The behaviors live in those views; only the routing is the view's.
public sealed partial class MainView : UserControl
{
    // Ctrl+F focuses the search box from anywhere in the window.
    private void OnFocusSearchAccelerator(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        _libraryBrowser.FocusSearch();
        args.Handled = true;
    }

    // Ctrl+L jumps to whatever's playing: open its album's detail and scroll the
    // track into view, flashing it. No-op when nothing is playing.
    private async void OnGoToNowPlaying(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        await _nowPlayingBar.GoToNowPlaying();
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

    // Space toggles play/pause from anywhere — except while typing in a text
    // field, where space must insert a space. Handled here, not as a button
    // accelerator, so a bare Space key never steals input from a text box.
    // Dialog/flyout text inputs are safe for free: they live in separate popups,
    // not under this root Grid, so their KeyDown never bubbles here. The focus
    // check only has to cover text inputs in the main tree — the search box and
    // the welcome chooser's restore-code box. Escape and Ctrl+A route to the
    // queue pane and the library browser, which own those behaviors.
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

        if (e.Key == VirtualKey.Escape && _libraryBrowser.HandleEscapeSelection())
        {
            e.Handled = true;
            return;
        }

        var focused = FocusManager.GetFocusedElement(XamlRoot);
        var focusedTextInput = focused is TextBox || focused is AutoSuggestBox;

        // Ctrl+A selects every loaded album, guarded by the same focused-text-input
        // check as Space so Ctrl+A in the search box still selects its text; the
        // browser owns the Ctrl-held and album-grid-visible checks.
        if (e.Key == VirtualKey.A && !focusedTextInput && _libraryBrowser.HandleSelectAll())
        {
            e.Handled = true;
            return;
        }

        if (e.Key != VirtualKey.Space || focusedTextInput)
        {
            return;
        }

        if (CurrentHandleOrNull() != null)
        {
            _nowPlayingBar.TogglePlayPause();
            e.Handled = true;
        }
    }
}
