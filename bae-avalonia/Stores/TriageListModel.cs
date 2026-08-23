using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The sidebar row list's sort order. Over the folder's path below its watched
// root, not over the row's title: for a candidate the user picked a release
// for, the title lives in an archived document, and ordering by it would mean
// decoding every pick on every read. Only name order survives the triage
// redesign — a row carries no discovery timestamp, so a "date added" option
// would silently degrade into an alias for this one.
internal enum CandidateSortOrder
{
    NameAZ,
    NameZA,
}

// What is left for the UI to say about a row once core has placed it: the title
// it leads with, and the persisted sort token. Which tab a row belongs to, which
// group it joins, whether the filter keeps it and where it sits are all core's,
// and arrive already decided on the list's items.
internal static class TriageListModel
{
    // The title a row leads with — the matched release's, or the folder name
    // when nothing matched. The row's own text, formatted by the UI.
    internal static string DisplayTitle(BridgeTriageRow row) => row.Matched?.Title ?? row.FolderName;

    // Whether DisplayTitle fell through to the folder name — the rows that take
    // a folder glyph, so the title reads as a place on disk rather than a
    // release nobody has matched.
    internal static bool TitleIsFolderName(BridgeTriageRow row) => row.Matched is null;

    // The order core reads the persisted preference as.
    internal static BridgeImportListOrder ListOrder(CandidateSortOrder order) => order switch
    {
        CandidateSortOrder.NameAZ => BridgeImportListOrder.PathAscending,
        CandidateSortOrder.NameZA => BridgeImportListOrder.PathDescending,
        _ => throw new ArgumentOutOfRangeException(nameof(order), order, "Unknown sort order"),
    };

    // Round-trip tokens for the persisted sort preference.
    internal static string Serialize(CandidateSortOrder order) => order switch
    {
        CandidateSortOrder.NameAZ => "nameAZ",
        CandidateSortOrder.NameZA => "nameZA",
        _ => throw new ArgumentOutOfRangeException(nameof(order), order, "Unknown sort order"),
    };

    // An unknown or absent token falls back to the default order — a sort
    // preference degrades to the default rather than failing.
    internal static CandidateSortOrder ParseSortOrder(string? token) => token switch
    {
        "nameAZ" => CandidateSortOrder.NameAZ,
        "nameZA" => CandidateSortOrder.NameZA,
        _ => CandidateSortOrder.NameAZ,
    };
}
