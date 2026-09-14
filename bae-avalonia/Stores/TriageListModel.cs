using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// What a candidate row says about its release, in three steps: nothing yet, a
// draft, or a draft a source's release was read into.
//
// Unidentified is exactly a row with no MetadataSummary: core leaves that
// absent when the draft is blank and no source has been applied.
internal abstract record TriageRowReading
{
    // Nothing has been written about the release, from tags or anywhere.
    internal sealed record Unidentified : TriageRowReading;

    // A draft read off the files' tags, or typed in.
    internal sealed record Prefilled : TriageRowReading;

    // A draft read from a source's release, naming every source the pick
    // claims.
    internal sealed record Identified(
        IReadOnlyList<BridgeMetadataSource> Sources) : TriageRowReading;
}

// What is left for the UI to say about a row once core has placed it: the title
// it leads with, and the persisted sort token. Which tab a row belongs to, which
// group it joins, whether the filter keeps it and where it sits are all core's,
// and arrive already decided on the list's items.
internal static class TriageListModel
{
    // The title a row leads with — the draft's album title, or the folder name
    // when there is no draft to lead with. The row's own text, formatted by
    // the UI.
    internal static string DisplayTitle(BridgeTriageRow row) =>
        row.MetadataSummary?.AlbumTitle is { Length: > 0 } title
            ? title
            : row.FolderName;

    // How the row reads. The sources a pick claims come back in the one order
    // surfaces list them in, which core states, so this never fixes an order
    // of its own.
    internal static TriageRowReading Reading(BridgeTriageRow row)
    {
        if (row.MetadataSummary is null)
        {
            return new TriageRowReading.Unidentified();
        }
        if (row.MetadataProvenance
            is not BridgeMetadataProvenance.ExternalRelease pick)
        {
            return new TriageRowReading.Prefilled();
        }
        var claimed = pick.Partners
            .Select(partner => partner.Source)
            .Append(pick.Source)
            .ToHashSet();
        return new TriageRowReading.Identified(
            BaeBridgeMethods.BridgeMetadataSources()
                .Where(claimed.Contains)
                .ToList());
    }

    // Round-trip tokens for the persisted sort preference.
    internal static string Serialize(BridgeImportListOrder order) => order switch
    {
        BridgeImportListOrder.NewestFirst => "newestFirst",
        BridgeImportListOrder.OldestFirst => "oldestFirst",
        BridgeImportListOrder.PathAscending => "nameAZ",
        BridgeImportListOrder.PathDescending => "nameZA",
        _ => throw new ArgumentOutOfRangeException(nameof(order), order, "Unknown sort order"),
    };

    internal static BridgeImportListOrder ParseSortOrder(string? token) => token switch
    {
        null => BridgeImportListOrder.NewestFirst,
        "newestFirst" => BridgeImportListOrder.NewestFirst,
        "oldestFirst" => BridgeImportListOrder.OldestFirst,
        "nameAZ" => BridgeImportListOrder.PathAscending,
        "nameZA" => BridgeImportListOrder.PathDescending,
        _ => throw new FormatException($"Unknown import sort preference: {token}"),
    };
}
