using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What a candidate row's folder states, and which catalogs describe the
/// release its draft was read from — the names first, then the catalogs, each
/// linking to its own page.
/// </summary>
internal static class IdentifiedFromFlyout
{
    internal static Control Build(
        IReadOnlyList<BridgeReleaseMark> marks,
        BridgeVerification? verification,
        IReadOnlyList<BridgeReleaseRecord> records)
    {
        var column = new StackPanel { Spacing = 10, Width = 276 };
        if (marks.Count > 0 || verification is not null)
        {
            column.Children.Add(RipMatchLine.BuildWithMarks(marks, verification));
        }
        var catalogs = new StackPanel { Spacing = 6 };
        var header = new TextBlock
        {
            Text = Loc.Core("core.import.triage.identified_from").ToUpperInvariant(),
            FontSize = 10,
            FontWeight = FontWeight.Bold,
            Margin = new Thickness(0, 0, 0, 1),
        };
        header[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        catalogs.Children.Add(header);
        catalogs.Children.Add(ReleaseRecordsRow.Build(records));
        column.Children.Add(catalogs);
        return new Border { Padding = new Thickness(12, 10), Child = column };
    }
}
