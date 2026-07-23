#if DEBUG
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// MainWindow: the debug-only toolbar entry that opens the component gallery (the
// WinUI preview analogue). Compiled only in DEBUG builds; AddDebugMenu is called
// from the constructor under the same #if.
public sealed partial class MainWindow
{
    private ComponentGalleryWindow? _componentGallery;

    // Add a debug toolbar button whose flyout opens the component gallery. Placed
    // at the leading edge of the toolbar's button strip so it doesn't shift the
    // shipping chrome.
    private void AddDebugMenu()
    {
        var item = new MenuFlyoutItem { Text = Loc.Chrome("component_gallery.menu") };
        item.Click += (_, _) => ShowComponentGallery();
        var flyout = new MenuFlyout();
        flyout.Items.Add(item);

        var button = new Button
        {
            Style = (Style)Application.Current.Resources["ToolbarIconButtonStyle"],
            // Segoe MDL2 Assets "Developer" glyph (U+EBE8).
            Content = new FontIcon { Glyph = "", FontSize = 16 },
            Flyout = flyout,
        };
        ToolTipService.SetToolTip(button, Loc.Chrome("component_gallery.menu"));
        ToolbarButtons.Children.Insert(0, button);
    }

    private void ShowComponentGallery()
    {
        if (_componentGallery is { } open)
        {
            open.Activate();
            return;
        }
        var window = new ComponentGalleryWindow();
        _componentGallery = window;
        window.Closed += (_, _) => _componentGallery = null;
        window.Activate();
        // AppWindow speaks physical pixels, so scale by the live rasterization
        // factor (available once activated), matching SettingsWindow.
        var scale = window.Content.XamlRoot?.RasterizationScale ?? 1.0;
        window.AppWindow.ResizeClient(new global::Windows.Graphics.SizeInt32(
            (int)(920 * scale), (int)(640 * scale)));
    }
}
#endif
