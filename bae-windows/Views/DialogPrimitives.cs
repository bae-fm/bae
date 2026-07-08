using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// Shared dialog building blocks used across the library-lifecycle, import, and
// album-detail screens.
internal static class DialogPrimitives
{
    // A dismiss-only error dialog: a title, an optional detail body, and a single
    // "OK" button that closes it.
    internal static async System.Threading.Tasks.Task ShowError(XamlRoot? xamlRoot, string title, string? message = null)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            CloseButtonText = Loc.Chrome("action.ok"),
            XamlRoot = xamlRoot,
        };
        if (message is not null)
        {
            dialog.Content = message;
        }

        await dialog.ShowAsync();
    }

    // A cover-art thumbnail tile: an image over a one-line caption, the whole tile
    // a borderless button. The caller wires Click — the change-cover gallery
    // applies the selection immediately, the import-confirm gallery carries it and
    // highlights the picked tile via the button's border.
    internal static Button CoverTile(ImageSource? source, string caption)
    {
        var thumb = new Image
        {
            Source = source,
            Stretch = Stretch.UniformToFill,
            Width = 120,
            Height = 120,
        };
        var label = new TextBlock
        {
            Text = caption,
            TextTrimming = TextTrimming.CharacterEllipsis,
            MaxWidth = 120,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        var stack = new StackPanel { Spacing = 4 };
        stack.Children.Add(thumb);
        stack.Children.Add(label);
        return new Button
        {
            Content = stack,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4),
        };
    }

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
