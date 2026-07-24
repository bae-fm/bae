using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// MainWindow: the Library/Import section switcher (the title bar's segmented
// pill, desktop story 3). The library section is the browse content column and
// its empty state; the import section hosts ImportSection's content. Switching
// toggles visibility only — both sections keep their state.
public sealed partial class MainWindow : Window
{
    private bool _importSectionActive;

    private void OnLibrarySegmentClick(object sender, RoutedEventArgs e) => SwitchSection(import: false);

    private void OnImportSegmentClick(object sender, RoutedEventArgs e) => SwitchSection(import: true);

    internal void SwitchSection(bool import)
    {
        _importSectionActive = import;
        if (import)
        {
            _importSection.Attach(ImportSectionHost);
            _importSection.OnEntered();
        }

        ContentColumn.Visibility = import ? Visibility.Collapsed : Visibility.Visible;
        EmptyState.Visibility = import ? Visibility.Collapsed : Visibility.Visible;
        ImportSectionHost.Visibility = import ? Visibility.Visible : Visibility.Collapsed;
        StyleSectionPill();
    }

    // The active segment fills with the accent over white bold text; the
    // inactive one sits flat on the pill's field. Mirrors the macOS switcher.
    private void StyleSectionPill()
    {
        StyleSegment(LibrarySegment, active: !_importSectionActive);
        StyleSegment(ImportSegment, active: _importSectionActive);
    }

    private static void StyleSegment(Button segment, bool active)
    {
        var accent = (global::Windows.UI.Color)Application.Current.Resources["SystemAccentColor"];
        segment.Background = active
            ? new SolidColorBrush(accent)
            : new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        segment.Foreground = active
            ? new SolidColorBrush(Microsoft.UI.Colors.White)
            : (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
    }

    // The library empty state (story 3): the current browse mode's title plus
    // the shared guidance line, shown only when that mode has nothing in it.
    private void RenderEmptyState(string? titleKey)
    {
        if (titleKey is null)
        {
            EmptyStatePanel.Visibility = Visibility.Collapsed;
            return;
        }

        EmptyTitle.Text = Loc.Chrome(titleKey);
        EmptyGuidance.Text = Loc.Chrome("library.empty_guidance");
        EmptyStatePanel.Visibility = Visibility.Visible;
    }

    private void OnCloseLibraryAccelerator(
        KeyboardAccelerator sender,
        KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        OnCloseLibraryClick(sender, new RoutedEventArgs());
    }

    private void OnStorageAccelerator(
        KeyboardAccelerator sender,
        KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        OnStorageClick(sender, new RoutedEventArgs());
    }
}
