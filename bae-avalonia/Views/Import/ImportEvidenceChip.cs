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
/// The chip that says a signal read off this file is what identified the
/// release: the image a barcode was OCR'd from, the log or sheet a disc ID was
/// computed from. It states a fact and does nothing.
///
/// The hover carries the whole sentence, value included — the chip is only as
/// wide as its surface allows.
/// </summary>
internal static class ImportEvidence
{
    /// <summary>The evidence this file is the source of, if it is any.</summary>
    internal static BridgeFileEvidence? Of(
        string fileId,
        IReadOnlyList<BridgeFileEvidence> evidence) =>
        evidence.FirstOrDefault(found => found.FileId == fileId);

    /// <summary>What hovering the file says, in the user's language.</summary>
    internal static string HoverText(BridgeFileEvidence evidence) =>
        Loc.Core(
            BaeBridgeMethods.BridgeFileEvidenceKey(evidence),
            "value",
            evidence.Value);

    /// <summary>The signal's own name, the same one the signals row uses.</summary>
    private static string Label(BridgeEvidenceSignal signal) => signal switch
    {
        BridgeEvidenceSignal.Barcode => Loc.Chrome("signal.kind.barcode"),
        _ => Loc.Chrome("signal.kind.disc_id"),
    };

    /// <summary>
    /// The chip itself. <paramref name="onImage"/> is for a thumbnail's corner,
    /// where it sits on a photograph rather than on the pane: it fills instead
    /// of tinting, so it reads against whatever is behind it, and gives up its
    /// label before it outgrows the tile.
    /// </summary>
    internal static Control Chip(BridgeFileEvidence evidence, bool onImage = false)
    {
        var text = new TextBlock
        {
            Text = Label(evidence.Signal),
            FontSize = 11,
            FontWeight = FontWeight.SemiBold,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var chip = new Border
        {
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(6, 1),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Center,
            Child = text,
        };
        if (onImage)
        {
            text.Foreground = Brushes.White;
            chip[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        }
        else
        {
            text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
            chip[!Border.BackgroundProperty] =
                new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        ToolTip.SetTip(chip, HoverText(evidence));
        return chip;
    }
}
