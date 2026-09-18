using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The dot after one release field's value, saying that the value is the
/// person's own or that the catalogs describing the release do not agree on
/// it. Which of the two it says — and whether it says anything at all — is
/// core's answer; this draws it and hovers the readings behind it.
/// </summary>
internal static class FieldOriginDot
{
    private const double Diameter = 5;

    /// <summary>The dot for this field, or <c>null</c> where core marked
    /// none.</summary>
    internal static Control? For(BridgeFieldProvenance provenance)
    {
        if (provenance.Dot is not { } dot)
        {
            return null;
        }
        var circle = new Ellipse
        {
            Width = Diameter,
            Height = Diameter,
            VerticalAlignment = VerticalAlignment.Center,
            Margin = new Thickness(6, 0, 0, 0),
            Name = $"origin-dot-{BaeBridgeMethods.BridgeFieldName(provenance.Field)}",
        };
        circle[!Shape.FillProperty] = new DynamicResourceExtension(
            dot == BridgeFieldDot.Disagreement
                ? "BaeAccentBrush"
                : "BaeTextSecondaryBrush");
        HoverFlyout.Attach(circle, () => Lines(provenance));
        return circle;
    }

    /// <summary>What stands behind the dot: a line per catalog describing the
    /// release with what that catalog states, and a line for where the value
    /// in the field itself came from when it was not a catalog.</summary>
    internal static Control Lines(BridgeFieldProvenance provenance)
    {
        var column = new StackPanel { Spacing = 6, MinWidth = 200 };
        foreach (var claim in provenance.Claims)
        {
            column.Children.Add(Line(
                BaeBridgeMethods.BridgeCatalogName(claim.Catalog),
                // An editable value's blank is the person's to fill; a catalog
                // stating nothing is a fact about the catalog, and the dash is
                // how the form writes it everywhere else.
                claim.Value ?? "—"));
        }
        var origin = provenance.Origin switch
        {
            BridgeFieldOrigin.Typed => Loc.Core("core.field.origin.typed"),
            BridgeFieldOrigin.Tags => Loc.Core("core.field.origin.tags"),
            // The catalog's own line already says what it states, so the value
            // being read from it adds nothing, and a blank field came from
            // nowhere.
            _ => null,
        };
        if (origin is not null)
        {
            var text = new TextBlock { Text = origin, FontSize = 11.5 };
            text[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            column.Children.Add(text);
        }
        return new Border { Padding = new Thickness(12, 10), Child = column };
    }

    private static Control Line(string catalog, string value)
    {
        var name = new TextBlock { Text = catalog, FontSize = 11.5 };
        name[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        var reading = new TextBlock
        {
            Text = value,
            FontSize = 11.5,
            HorizontalAlignment = HorizontalAlignment.Right,
            TextTrimming = Avalonia.Media.TextTrimming.CharacterEllipsis,
        };
        var row = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("Auto,*"),
            ColumnSpacing = 8,
        };
        Grid.SetColumn(name, 0);
        Grid.SetColumn(reading, 1);
        row.Children.Add(name);
        row.Children.Add(reading);
        return row;
    }
}
