using System;
using System.Collections.Generic;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// What an import records, stated as a sentence: the pressing you claim to
/// physically hold, with a muted note saying what identified it.
/// </summary>
internal static class ClaimLineView
{
    internal static Control Build(BridgeClaimLine claim)
    {
        var sentence = new TextBlock { Text = ClaimSentence(claim), VerticalAlignment = VerticalAlignment.Center };
        var evidence = new TextBlock { Text = EvidenceNote(claim), FontSize = 12, VerticalAlignment = VerticalAlignment.Center };
        evidence[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var top = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        top.Children.Add(sentence);
        top.Children.Add(evidence);

        var column = new StackPanel { Spacing = 2 };
        column.Children.Add(top);
        return column;
    }

    /// <summary>"You have this pressing — CD · 2004 · UK · CAT-1234".</summary>
    private static string ClaimSentence(BridgeClaimLine claim)
    {
        return claim.Release is { } release
            ? Loc.Core("ui.import.claim.pressing", "release", release)
            : Loc.Core("ui.import.claim.pressing_undescribed");
    }

    private static string EvidenceNote(BridgeClaimLine claim) => claim.Evidence switch
    {
        BridgeClaimEvidence.DiscIdAlone => Loc.Core("ui.import.claim.evidence.disc_id"),
        BridgeClaimEvidence.DiscIdShared shared =>
            Loc.Core("ui.import.claim.evidence.disc_id_shared", "count", checked((int)shared.MatchCount)),
        BridgeClaimEvidence.Barcode => Loc.Core("ui.import.claim.evidence.barcode"),
        _ => Loc.Core("ui.import.claim.evidence.search"),
    };
}
