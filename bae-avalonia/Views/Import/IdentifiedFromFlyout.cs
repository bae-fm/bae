using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Which catalogs describe the release a candidate row's draft was read from,
/// each linking to its own page for it.
/// </summary>
internal static class IdentifiedFromFlyout
{
    internal static Control Build(IReadOnlyList<BridgeReleaseRecord> records)
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
        column.Children.Add(ReleaseRecordsRow.Build(records));
        return new Border { Padding = new Thickness(12, 10), Child = column };
    }
}
