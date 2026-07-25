using Avalonia;
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

    // A section eyebrow over a group of dialog content.
    internal static TextBlock SectionLabel(string text)
    {
        var t = new TextBlock { Text = text, FontSize = 13, FontWeight = FontWeight.SemiBold };
        t[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return t;
    }

    // A cover-art thumbnail tile: a 120×120 image over a one-line caption, the whole
    // tile a borderless button. The caller sets the image source (sync for a local
    // file, async for a remote candidate) and wires Click.
    internal static Button CoverTile(Image image, string caption)
    {
        image.Width = 120;
        image.Height = 120;
        image.Stretch = Stretch.UniformToFill;
        var thumb = new Border { CornerRadius = new CornerRadius(6), ClipToBounds = true, Child = image };
        thumb[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        var label = new TextBlock
        {
            Text = caption,
            FontSize = 12,
            MaxWidth = 120,
            TextTrimming = TextTrimming.CharacterEllipsis,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return new Button
        {
            Padding = new Thickness(4),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Content = new StackPanel { Spacing = 4, Children = { thumb, label } },
        };
    }

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
