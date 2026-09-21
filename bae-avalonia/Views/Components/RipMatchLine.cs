using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What the rip databases said about a release's audio: how many other
/// people's copies of the same disc carry the same bits.
///
/// Core folds the tracks into that one number — the weakest track's best
/// database — so this draws what it is given. A release whose every track no
/// database confirmed has no number and no line.
/// </summary>
internal static class RipMatchLine
{
    /// <summary>The line, or <c>null</c> where there is no count to state.</summary>
    internal static Control? Build(BridgeVerification? verification, Action<BridgeEvidenceSelection>? openEvidence = null)
    {
        if (verification?.MatchedCopies is not { } matchedCopies)
        {
            return null;
        }
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 7,
        };
        Avalonia.Automation.AutomationProperties.SetAutomationId(row, "rip-match");
        var check = Icons.Glyph(Icons.Check, 12, "BaeSuccessBrush");
        check.VerticalAlignment = VerticalAlignment.Center;
        Avalonia.Automation.AutomationProperties.SetName(
            check, Loc.Core("core.identity.verified"));
        row.Children.Add(check);

        var text = new TextBlock
        {
            Text = Loc.Core(
                "core.verification.matches_other_rips",
                "count",
                (long)matchedCopies),
            FontSize = 11.5,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextPrimaryBrush");
        row.Children.Add(text);
        row.Children.Add(EvidenceChip.Build(
            Loc.Core(BaeBridgeMethods.BridgeSignalOriginKey(BridgeSignalOrigin.DiscToc)),
            new BridgeEvidenceSelection.Verification(), openEvidence));
        return row;
    }
}
