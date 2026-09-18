using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What a release says about itself: the names read off the object, what the
/// rip databases said about its audio, then every catalog that describes it.
/// Either half is absent when there is nothing of it to state, and the
/// hairline between them goes with whichever is missing.
///
/// One card wherever the question is asked — the candidate row's glyphs and
/// the library expansion's facts line — because it is one answer.
/// </summary>
internal static class ReleaseFactsFlyout
{
    internal const double Width = 320;

    internal static Control Build(
        IReadOnlyList<BridgeReleaseMark> marks,
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
        var statesNames = marks.Count > 0 || verification?.MatchedCopies is not null;
        if (statesNames)
        {
            body.Children.Add(
                RipMatchLine.BuildWithMarks(marks, verification, ReleaseFactsScale.Card, openEvidence));
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
