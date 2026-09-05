using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// What is left for the UI to say about a row once core has placed it: the title
// it leads with, and the persisted sort token. Which tab a row belongs to, which
// group it joins, whether the filter keeps it and where it sits are all core's,
// and arrive already decided on the list's items.
internal static class TriageListModel
{
    // The title a row leads with — the matched release's, or the folder name
    // when nothing matched. The row's own text, formatted by the UI.
    internal static string DisplayTitle(BridgeTriageRow row) =>
        row.MetadataSummary?.AlbumTitle is { Length: > 0 } title
            ? title
            : row.Matched?.Title ?? row.FolderName;

    // Whether DisplayTitle fell through to the folder name — the rows that take
    // a folder glyph, so the title reads as a place on disk rather than a
    // release nobody has matched.
    internal static bool TitleIsFolderName(BridgeTriageRow row) =>
        row.MetadataSummary is null && row.Matched is null;

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
