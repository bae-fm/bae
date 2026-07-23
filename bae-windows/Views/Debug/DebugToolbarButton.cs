#if DEBUG
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Bae.Windows;

// DEBUG-only: the leading toolbar entry that opens the component gallery (the
// WinUI preview analogue). Owns the single gallery-window instance so a second
// click re-activates the open one rather than opening another. Attached to the
// toolbar's button strip from MainWindow's composition root under the same #if.
// Compiled only in DEBUG builds.
internal sealed class DebugToolbarButton
{
    private ComponentGalleryWindow? _gallery;

    // Add the gallery button at the leading edge of the toolbar's button strip so
    // it doesn't shift the shipping chrome. The button's flyout item captures the
    // owner (which holds the single-instance window), so the owner lives as long
    // as the button does.
    public static void Attach(Panel toolbarButtons)
    {
        var owner = new DebugToolbarButton();

        var item = new MenuFlyoutItem { Text = Loc.Chrome("component_gallery.menu") };
        item.Click += (_, _) => owner.Show();
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
        toolbarButtons.Children.Insert(0, button);
    }

    private void Show()
    {
        if (_gallery is { } open)
        {
            open.Activate();
            return;
        }
        var window = new ComponentGalleryWindow();
        _gallery = window;
        window.Closed += (_, _) => _gallery = null;
        window.Activate();
        // AppWindow speaks physical pixels, so scale by the live rasterization
        // factor (available once activated), matching SettingsWindow.
        var scale = window.Content.XamlRoot?.RasterizationScale ?? 1.0;
        window.AppWindow.ResizeClient(new global::Windows.Graphics.SizeInt32(
            (int)(920 * scale), (int)(640 * scale)));
    }
}
#endif
