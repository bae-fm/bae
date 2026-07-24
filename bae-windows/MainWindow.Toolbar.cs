using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using uniffi.bae_bridge;

namespace Bae.Windows;

// MainWindow: the toolbar strip's button clicks. Each is a one-line forward from
// a shell-chrome button to the composition-root dialog/window it opens (or the
// library op it runs) — the XAML Click handlers the shell skeleton wires. The
// features behind them live in their own views; only this forwarding stays.
public sealed partial class MainWindow : Window
{
    // Give an icon-only chrome button a fixed accessible name and matching tooltip
    // from a chrome catalog key. Used for the toolbar buttons and the now-playing
    // bar's fixed-meaning icons (prev/next/queue), set from the composition root.
    private static void SetIconButtonLabel(Button button, string key)
    {
        var label = Loc.Chrome(key);
        AutomationProperties.SetName(button, label);
        ToolTipService.SetToolTip(button, label);
    }

    private async void OnLibrariesClick(object sender, RoutedEventArgs e) => await _librariesDialog.Show();

    private async void OnImportClick(object sender, RoutedEventArgs e) => await _importDialog.Show();

    private async void OnStorageClick(object sender, RoutedEventArgs e) => await _storageDialog.Show();

    private void OnSettingsClick(object sender, RoutedEventArgs e) => _settingsWindow.Show();

    private void OnShuffleLibraryClick(object sender, RoutedEventArgs e) =>
        WithCurrentHandle(NativeBae.PlayLibraryShuffled);

    private async void OnCloseLibraryClick(object sender, RoutedEventArgs e) => await _closeToWelcome();
}
