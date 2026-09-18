using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Every catalog that describes a release, as a wrapped row of links.
///
/// One entry per catalog, in the order core lists them, each opening that
/// catalog's page for this release. The name is the catalog's own brand and the
/// address is built by core, so this row neither translates nor composes
/// anything — it draws what the records say.
/// </summary>
internal static class ReleaseRecordsRow
{
    internal static Control Build(
        IReadOnlyList<BridgeReleaseRecord> records,
        ReleaseFactsScale scale = ReleaseFactsScale.Pane)
    {
        var row = new WrapPanel
        {
            Orientation = Orientation.Horizontal,
            ItemSpacing = scale.RecordSpacing(),
            LineSpacing = scale.RecordRowSpacing(),
        };
        Avalonia.Automation.AutomationProperties.SetAutomationId(row, "release-records");
        foreach (var record in records)
        {
            row.Children.Add(Link(record, scale.RecordFontSize()));
        }
        return row;
    }

    private static Control Link(BridgeReleaseRecord record, double fontSize)
    {
        var name = new TextBlock
        {
            Text = BaeBridgeMethods.BridgeCatalogName(record.Catalog)
                + " "
                + ImportPaneUi.OutboundArrow,
            FontSize = fontSize,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        name[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeAccentBrush");
        var button = new Button
        {
            Content = name,
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            Background = Brushes.Transparent,
            Cursor = new Avalonia.Input.Cursor(
                Avalonia.Input.StandardCursorType.Hand),
        };
        var uri = new Uri(record.Url);
        button.Click += async (_, _) => await ImportPaneUi.OpenExternal(button, uri);
        return button;
    }
}
