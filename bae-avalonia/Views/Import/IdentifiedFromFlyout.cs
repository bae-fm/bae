using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Which sources a candidate row's draft was read from, and what each of their
/// releases says about itself.
///
/// Both lines when the pick paired both sources: a MusicBrainz release and a
/// Discogs release describing one pressing can disagree about its label and its
/// year, so each line states its own source's document rather than the draft
/// they were merged into.
/// </summary>
internal static class IdentifiedFromFlyout
{
    internal static Control Build(IReadOnlyList<BridgeIdentifiedSource> sources)
    {
        var column = new StackPanel { Spacing = 6, Width = 276 };
        var header = new TextBlock
        {
            Text = Loc.Core("core.import.triage.identified_from").ToUpperInvariant(),
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Margin = new Thickness(0, 0, 0, 1),
        };
        header[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        column.Children.Add(header);
        foreach (var source in sources)
        {
            column.Children.Add(Line(source));
        }
        return new Border { Padding = new Thickness(12, 10), Child = column };
    }

    private static Control Line(BridgeIdentifiedSource source)
    {
        var row = new Grid
        {
            // The source is what the line names, so its column is sized to
            // it; a long label's facts take what is left and trim.
            ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto"),
            ColumnSpacing = 6,
        };

        var name = new TextBlock
        {
            Text = BaeBridgeMethods.BridgeMetadataSourceName(source.Source),
            FontSize = 12,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        name[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextPrimaryBrush");
        Grid.SetColumn(name, 0);
        row.Children.Add(name);

        if (Facts(source) is { } facts)
        {
            var stated = new TextBlock
            {
                Text = facts,
                FontFamily = new FontFamily("monospace"),
                FontSize = 10.5,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                TextAlignment = TextAlignment.Right,
                VerticalAlignment = VerticalAlignment.Center,
            };
            stated[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            Grid.SetColumn(stated, 1);
            row.Children.Add(stated);
        }

        var arrow = new TextBlock
        {
            Text = ImportPaneUi.OutboundArrow,
            FontSize = 10,
            VerticalAlignment = VerticalAlignment.Center,
        };
        arrow[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeAccentBrush");
        var link = new Button
        {
            Content = arrow,
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            Background = Brushes.Transparent,
            Cursor = new Avalonia.Input.Cursor(
                Avalonia.Input.StandardCursorType.Hand),
            VerticalAlignment = VerticalAlignment.Center,
        };
        var uri = new Uri(source.Url);
        link.Click += async (_, _) => await ImportPaneUi.OpenExternal(link, uri);
        Grid.SetColumn(link, 2);
        row.Children.Add(link);

        return row;
    }

    /// <summary>The label and the year this source's own release states,
    /// whichever of them it states. Null when it states neither — the line is
    /// then the source's name and the way to it.</summary>
    private static string? Facts(BridgeIdentifiedSource source)
    {
        var stated = new[] { source.Label, source.Year?.ToString() }
            .Where(part => !string.IsNullOrEmpty(part))
            .ToArray();
        return stated.Length == 0 ? null : string.Join(" · ", stated);
    }
}
