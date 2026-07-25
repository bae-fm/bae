using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// Small building blocks shared by the modal-host dialogs, so each reads as a
// column of labeled fields with a themed action row. Every color is a theme
// brush; no dialog carries a literal.
internal static class DialogUi
{
    internal static TextBlock Title(string text)
    {
        var t = new TextBlock { Text = text, FontSize = 20, FontWeight = FontWeight.SemiBold };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        return t;
    }

    internal static TextBlock Body(string text)
    {
        var t = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return t;
    }

    internal static TextBlock Danger()
    {
        var t = new TextBlock { TextWrapping = TextWrapping.Wrap, IsVisible = false };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
        return t;
    }

    // A labeled text field; secret masks its input.
    internal static Control Field(string label, out TextBox box, bool secret = false)
    {
        box = new TextBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        if (secret)
        {
            box.PasswordChar = '•';
        }
        var caption = new TextBlock { Text = label, FontSize = 12.5 };
        caption[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return new StackPanel { Spacing = 4, Children = { caption, box } };
    }

    internal static Button Primary(string text)
    {
        var b = new Button { Content = text, HorizontalContentAlignment = HorizontalAlignment.Center };
        b.Classes.Add("accent");
        return b;
    }

    internal static StackPanel Column() =>
        new() { Spacing = 12, MinWidth = 360 };

    internal static Control Actions(params Control[] buttons)
    {
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        foreach (var b in buttons)
        {
            row.Children.Add(b);
        }
        return row;
    }
}
