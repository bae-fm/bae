using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static class EvidenceChip
{
    internal static Button Build(string label, BridgeEvidenceSelection selection,
        Action<BridgeEvidenceSelection>? open)
    {
        var text = new TextBlock { Text = label, FontSize = 9.5 };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var button = new Button
        {
            Content = text,
            Padding = new Thickness(5, 1),
            CornerRadius = new CornerRadius(4),
            BorderThickness = new Thickness(0),
            MinHeight = 0,
            VerticalAlignment = VerticalAlignment.Center,
        };
        button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeHoverBrush");
        button.Click += (_, args) =>
        {
            args.Handled = true;
            (open ?? throw new InvalidOperationException("Evidence chip has no reader"))(selection);
        };
        button.Tapped += (_, args) => args.Handled = true;
        return button;
    }
}
