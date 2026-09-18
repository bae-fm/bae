using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What a release says about itself: the names read off the object, what the
/// rip databases said about its audio, then every catalog that describes it.
/// Either half is absent when there is nothing of it to state.
///
/// One card wherever the question is asked — the candidate row's glyphs and
/// the library expansion's facts line — because it is one answer.
/// </summary>
internal static class ReleaseFactsFlyout
{
    internal static Control Build(
        IReadOnlyList<BridgeReleaseMark> marks,
        BridgeVerification? verification,
        IReadOnlyList<BridgeReleaseRecord> records)
    {
        var body = new StackPanel { Spacing = 10, Width = 276 };
        if (marks.Count > 0 || verification is not null)
        {
            body.Children.Add(RipMatchLine.BuildWithMarks(marks, verification));
        }
        if (records.Count > 0)
        {
            body.Children.Add(ReleaseRecordsRow.Build(records));
        }
        return new Border { Padding = new Thickness(12, 10), Child = body };
    }
}
