using Microsoft.UI.Xaml;
using uniffi.bae_bridge;

namespace Bae.Windows;

// MainWindow: the toolbar strip's button clicks. Each is a one-line forward from
// a shell-chrome button to the composition-root dialog/window it opens (or the
// library op it runs) — the XAML Click handlers the shell skeleton wires. The
// features behind them live in their own views; only this forwarding stays.
public sealed partial class MainWindow : Window
{
    private async void OnLibrariesClick(object sender, RoutedEventArgs e) => await _librariesDialog.Show();

    private async void OnImportClick(object sender, RoutedEventArgs e) => await _importDialog.Show();

    private async void OnStorageClick(object sender, RoutedEventArgs e) => await _storageDialog.Show();

    private void OnSettingsClick(object sender, RoutedEventArgs e) => _settingsWindow.Show();

    private void OnShuffleLibraryClick(object sender, RoutedEventArgs e) =>
        WithCurrentHandle(NativeBae.PlayLibraryShuffled);

    private async void OnCloseLibraryClick(object sender, RoutedEventArgs e) => await CloseLibrary();
}
