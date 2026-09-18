using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The names an object itself carries, one line each in the order core lists
/// mark kinds. A line is sealed only when its lookup found the chosen record.
/// It names the kind, value, and surfaces it was read from.
///
/// Core folds the readings — two scans showing one barcode arrive as one mark
/// tagged `scan` — so this draws what it is given.
/// </summary>
internal static class MarkLines
{
    /// <summary>How much of a mark's value the line shows before its middle
    /// gives way. A barcode and a catalog number fit whole; a disc ID does
    /// not, and its ends are what name it.</summary>
    private const int MarkValueChars = 20;

    internal static Control Build(
        IReadOnlyList<BridgeReleaseMark> marks,
        ReleaseFactsScale scale = ReleaseFactsScale.Pane)
    {
        var column = new StackPanel { Spacing = scale.LineSpacing() };
        Avalonia.Automation.AutomationProperties.SetAutomationId(column, "release-marks");
        foreach (var mark in marks)
        {
            column.Children.Add(Line(mark));
        }
        return column;
    }

    private static Control Line(BridgeReleaseMark mark)
    {
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 7,
        };
        var seal = Icons.Glyph(Icons.Seal, 12, "BaeTextSecondaryBrush");
        seal.VerticalAlignment = VerticalAlignment.Center;
        seal.Opacity = mark.Corroborated ? 1 : 0;
        seal.IsHitTestVisible = mark.Corroborated;
        Avalonia.Automation.AutomationProperties.SetAccessibilityView(
            seal, mark.Corroborated
                ? Avalonia.Automation.AccessibilityView.Content
                : Avalonia.Automation.AccessibilityView.Raw);
        Avalonia.Automation.AutomationProperties.SetName(
            seal, Loc.Core("core.identity.identified"));
        row.Children.Add(seal);

        var kind = new TextBlock
        {
            Text = Loc.Core(BaeBridgeMethods.BridgeMarkKindKey(mark.Kind)),
            FontSize = 11.5,
            VerticalAlignment = VerticalAlignment.Center,
        };
        kind[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        row.Children.Add(kind);

        // A disc ID is longer than the line it sits on; its head and tail are
        // what identifies it, so the middle is what goes. The toolkit trims
        // from the end only, so the value is middle-truncated before it is
        // set — the same budget the signal badges spend on a value.
        var value = new TextBlock
        {
            Text = TextTruncation.MiddleTruncate(mark.Value, MarkValueChars),
            FontSize = 11,
            FontFamily = new FontFamily("monospace"),
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        value[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextPrimaryBrush");
        row.Children.Add(value);

        foreach (var origin in mark.Origins)
        {
            row.Children.Add(OriginTag(origin));
        }
        return row;
    }

    private static Control OriginTag(BridgeSignalOrigin origin)
    {
        var text = new TextBlock
        {
            Text = Loc.Core(BaeBridgeMethods.BridgeSignalOriginKey(origin)),
            FontSize = 9.5,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        var tag = new Border
        {
            Padding = new Thickness(5, 1),
            CornerRadius = new CornerRadius(4),
            Child = text,
            VerticalAlignment = VerticalAlignment.Center,
        };
        tag[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeHoverBrush");
        return tag;
    }
}
