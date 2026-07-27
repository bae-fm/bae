using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// The two facts an import records, stated as a sentence: what you claim to
/// physically hold, and — when that is not the same release — where the
/// metadata was read from. Both come from <c>BridgeClaimLine</c>, which
/// bae-core derives from the evidence that identified the candidate; nothing
/// here decides anything, and there is no mode to switch. Picking a different
/// pressing is what moves the claim.
///
/// Shared by the import confirm dialog and the re-identify dialog, which take
/// the same identity claim.
/// </summary>
internal static class ClaimLineView
{
    internal static FrameworkElement Build(BridgeClaimLine claim)
    {
        var top = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 10,
            VerticalAlignment = VerticalAlignment.Center,
        };
        top.Children.Add(new TextBlock
        {
            Text = ClaimSentence(claim),
            VerticalAlignment = VerticalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
        });
        top.Children.Add(new TextBlock
        {
            Text = EvidenceNote(claim),
            FontSize = 12,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            VerticalAlignment = VerticalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
        });

        var column = new StackPanel { Spacing = 2 };
        column.Children.Add(top);
        if (claim.ShowsMetadataSource)
        {
            column.Children.Add(new TextBlock
            {
                Text = MetadataSourceLine(claim),
                FontSize = 12.5,
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                TextWrapping = TextWrapping.Wrap,
            });
        }
        return column;
    }

    /// <summary>"You have this pressing — CD · 2004 · UK · CAT-1234", or the
    /// album-level claim, which names no pressing because none is claimed.
    ///
    /// One fact drives both lines: a pressing claim names the picked release
    /// inside its own sentence, so there is no second line to draw, and an
    /// album claim needs one. bae-core decides it — the choice variant is
    /// carried for the commit, not for the view to re-read.</summary>
    private static string ClaimSentence(BridgeClaimLine claim)
    {
        if (claim.ShowsMetadataSource)
        {
            return Loc.Core("ui.import.claim.album");
        }
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

    /// <summary>"Metadata from 2015 · US · CD · 14 tracks". The description and
    /// the track count are separate messages so each stays a whole sentence its
    /// translators control; the "·" join is the one bae-core used inside the
    /// description.
    ///
    /// The description alone decides which sentence is used. A track count is
    /// not a name for a release — "Metadata from 14 tracks" would name nothing
    /// — so a release with no description takes the undescribed sentence and
    /// drops the count, however well known it is.</summary>
    private static string MetadataSourceLine(BridgeClaimLine claim)
    {
        if (claim.Release is not { } release)
        {
            return Loc.Core("ui.import.claim.metadata_from_undescribed");
        }
        var parts = new List<string> { release };
        if (claim.TrackCount is { } count)
        {
            parts.Add(Loc.Core("ui.import.claim.track_count", "count", checked((int)count)));
        }
        return Loc.Core("ui.import.claim.metadata_from", "release", string.Join(" · ", parts));
    }
}
