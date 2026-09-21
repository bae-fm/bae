using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What a release says about itself: what the rip databases said about its
/// audio, then every catalog that describes it. Either half is absent when
/// there is nothing of it to state, and the hairline between them goes with
/// whichever is missing.
/// </summary>
internal static class ReleaseFactsFlyout
{
    internal const double Width = 320;

    internal static Control Build(
        BridgeVerification? verification,
        IReadOnlyList<BridgeReleaseRecord> records,
        Action<BridgeEvidenceSelection>? openEvidence = null)
    {
        var padding = new Thickness(12, 10);
        var body = new StackPanel
        {
            Spacing = ReleaseFactsScale.Card.LineSpacing(),
            Width = Width - padding.Left - padding.Right,
        };
        var statesNames = verification?.MatchedCopies is not null;
        if (RipMatchLine.Build(verification, openEvidence) is { } line)
        {
            body.Children.Add(line);
        }
        if (statesNames && records.Count > 0)
        {
            body.Children.Add(new Separator());
        }
        if (records.Count > 0)
        {
            body.Children.Add(ReleaseRecordsRow.Build(records, ReleaseFactsScale.Card));
        }
        return new Border { Padding = padding, Child = body };
    }
}
