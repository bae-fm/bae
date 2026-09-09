using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Whole-value edits of what one candidate's identification asks about. A
/// control acts on one part of <see cref="BridgeLookupChoices"/> and sends the
/// whole value back, so the change each control makes is made here.
///
/// Value in, value out: nothing here reaches the library, which is why it
/// lives beside the bridge records rather than behind a domain service.
/// </summary>
internal static class LookupChoiceEdits
{
    /// <summary>What the candidate's identification asks about after the
    /// person acts on one badge: the whole value, with that one part turned
    /// over. A signal flips between left out and asked about; a catalog number
    /// joins the numbers the run looks up or leaves them. What the folder's
    /// text is taken to state about the answers is carried through untouched —
    /// no badge acts on it.</summary>
    internal static BridgeLookupChoices Toggling(
        BridgeLookupChoices current, string kind, string value)
    {
        switch (kind)
        {
            case "disc_id":
                return current with { DiscIdExcluded = !current.DiscIdExcluded };
            case "barcode":
                return current with { BarcodeExcluded = !current.BarcodeExcluded };
            default:
                var chosen = new List<string>(current.ChosenCatalogs);
                if (!chosen.Remove(value))
                {
                    chosen.Add(value);
                }
                return current with { ChosenCatalogs = [.. chosen] };
        }
    }

    /// <summary>The whole value with <paramref name="value"/> struck out of
    /// what the folder is taken to state, or counted again when it already was
    /// struck out. A set, so it goes back sorted and each number once.
    ///
    /// What the run looks up is untouched: striking a number out asks the
    /// providers nothing, it only says what their answers are ranked by.
    /// </summary>
    internal static BridgeLookupChoices Discounting(
        BridgeLookupChoices current, string value)
    {
        var struckOut = new SortedSet<string>(current.DiscountedCatalogs);
        if (!struckOut.Remove(value))
        {
            struckOut.Add(value);
        }
        return current with { DiscountedCatalogs = [.. struckOut] };
    }

    /// <summary>The choices a candidate nobody has touched runs with: nothing
    /// left out, nothing chosen, everything the folder states counted. What a
    /// session with no candidate row to store them on starts from.</summary>
    internal static BridgeLookupChoices Untouched() =>
        new(
            DiscIdExcluded: false,
            BarcodeExcluded: false,
            ChosenCatalogs: [],
            DiscountedCatalogs: []);
}
