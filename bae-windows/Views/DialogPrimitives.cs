using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// Shared dialog building blocks used across the library-lifecycle screens.
internal static class DialogPrimitives
{
    // A code-display block: the QR image (when it renders), the code as selectable
    // monospaced text, and a Copy button. Shared by the join screen (this device's
    // code), the approve flow (the invite code), and the recovery reveal.
    internal static StackPanel BuildCodeDisplay(string code)
    {
        var panel = new StackPanel { Spacing = 8, HorizontalAlignment = HorizontalAlignment.Center };

        var qr = QrCode.Image(code);
        if (qr is not null)
        {
            panel.Children.Add(new Image
            {
                Source = qr,
                Width = 180,
                Height = 180,
                Stretch = Stretch.Uniform,
            });
        }

        panel.Children.Add(new TextBox
        {
            Text = code,
            IsReadOnly = true,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("Consolas"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        });

        var copy = new Button
        {
            Content = Loc.Chrome("action.copy"),
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        copy.Click += (_, _) => ClipboardHelper.CopyToClipboard(code);
        panel.Children.Add(copy);

        return panel;
    }
}
