using System;
using System.Collections.Generic;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The two facts an import records, stated as a sentence with the claim on a
/// control: what you claim to physically hold, and — when that is not the same
/// release — where the metadata was read from. Picking a release claims that
/// pressing; the control is how you say you hold the album but can't vouch for
/// which pressing, and setting it re-picks the same release at the level
/// chosen, so the claim is stored rather than kept in the view.
///
/// Shared by the import confirm dialog and the re-identify dialog, which take
/// the same identity claim.
/// </summary>
internal static class ClaimLineView
{
    internal static Control Build(
        BridgeClaimLine claim,
        bool isReading,
        Action<BridgeClaimLevel> onSetLevel)
    {
        var sentence = new TextBlock { Text = ClaimSentence(claim), VerticalAlignment = VerticalAlignment.Center };
        var evidence = new TextBlock { Text = EvidenceNote(claim), FontSize = 12, VerticalAlignment = VerticalAlignment.Center };
        evidence[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var top = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        top.Children.Add(sentence);
        top.Children.Add(evidence);

        var column = new StackPanel { Spacing = 2 };
        column.Children.Add(top);
        column.Children.Add(LevelPicker(claim, isReading, onSetLevel));
        if (claim.Level is BridgeClaimLevel.Approximate)
        {
            var source = new TextBlock { Text = MetadataSourceLine(claim), FontSize = 12.5 };
            source[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            column.Children.Add(source);
        }
        return column;
    }

    /// <summary>The claim itself, as the one control that moves it. Both sides
    /// name the same picked release — lowering the claim says the pressing is
    /// not being vouched for, not that the release is wrong.</summary>
    private static Control LevelPicker(
        BridgeClaimLine claim,
        bool isReading,
        Action<BridgeClaimLevel> onSetLevel)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        row.Children.Add(Segment(
            Loc.Core("ui.import.claim.level.exact"), BridgeClaimLevel.Exact, claim.Level, isReading, onSetLevel));
        row.Children.Add(Segment(
            Loc.Core("ui.import.claim.level.album"), BridgeClaimLevel.Approximate, claim.Level, isReading, onSetLevel));
        ToolTip.SetTip(row, Loc.Core("ui.import.claim.level.title"));
        return row;
    }

    private static Control Segment(
        string label,
        BridgeClaimLevel level,
        BridgeClaimLevel current,
        bool isReading,
        Action<BridgeClaimLevel> onSetLevel)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = !isReading;
        if (current == level)
        {
            button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
            button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        }
        button.Click += (_, _) => onSetLevel(level);
        return button;
    }

    /// <summary>"You have this pressing — CD · 2004 · UK · CAT-1234", or the
    /// album-level claim, which names no pressing because none is claimed.
    ///
    /// The level drives both lines: a pressing claim names the picked release
    /// inside its own sentence, so there is no second line to draw, and an
    /// album claim needs one.</summary>
    private static string ClaimSentence(BridgeClaimLine claim)
    {
        if (claim.Level is BridgeClaimLevel.Approximate)
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
