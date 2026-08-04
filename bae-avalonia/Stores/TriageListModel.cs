using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The sidebar row list's sort order — the only thing left for the UI to decide
// once core has placed every row into its tab and, under Needs you, its group.
// Only name order survives the triage redesign: a BridgeTriageRow carries no
// discovery timestamp, so a "date added" option
// would silently degrade into an alias for name order. Better to drop it than
// keep a control that lies about what it does.
internal enum CandidateSortOrder
{
    NameAZ,
    NameZA,
}

internal sealed record ReleaseQueueEntry(
    string StableKey,
    BridgeTriageEntry Bridge);

internal sealed record ReleaseQueueSection(
    BridgeTriageTab Tab,
    string WatchedFolderPath,
    BridgeTriageGroup? Group,
    List<ReleaseQueueEntry> Entries);

// The sidebar's row list, grouping, filtering, and sort order, read off
// BridgeTriageQueue — core's projection. This computes nothing about which tab
// or Needs-you group a row belongs to (BridgeTriageRow.Placement already says
// that); it only filters, groups, and orders what core already placed.
internal static class TriageListModel
{
    internal static List<ReleaseQueueSection> Sections(
        BridgeTriageQueue queue,
        BridgeTriageTab tab,
        string filterText,
        CandidateSortOrder sortOrder)
    {
        var query = filterText.Trim().ToLowerInvariant();
        return queue.Sections
            .Where(section => section.Tab == tab)
            .Select(section => new ReleaseQueueSection(
                section.Tab,
                section.WatchedFolderPath,
                section.Group,
                ReleaseQueueSortModel.Sort(
                    section.Entries
                        .Where(entry => EntryMatches(entry, query))
                        .Select(entry => new ReleaseQueueEntry(
                            StableKey(entry),
                            entry)),
                    entry => EntryTitle(entry.Bridge),
                    sortOrder == CandidateSortOrder.NameZA)))
            .Where(section => section.Entries.Count > 0)
            .ToList();
    }

    private static string StableKey(BridgeTriageEntry entry) => entry switch
    {
        BridgeTriageEntry.Candidate candidate => candidate.StableKey,
        BridgeTriageEntry.Boundary boundary => boundary.StableKey,
        BridgeTriageEntry.Invalid invalid => invalid.StableKey,
        _ => throw new ArgumentOutOfRangeException(nameof(entry), entry, "Unknown triage entry"),
    };

    private static bool EntryMatches(BridgeTriageEntry entry, string query)
    {
        if (query.Length == 0)
        {
            return true;
        }
        return EntryTitle(entry).Contains(query, StringComparison.CurrentCultureIgnoreCase)
            || entry switch
            {
                BridgeTriageEntry.Candidate candidate =>
                    candidate.Row.DisplayPath.Contains(query, StringComparison.CurrentCultureIgnoreCase),
                BridgeTriageEntry.Boundary boundary =>
                    boundary.BoundaryValue.DisplayPath.Contains(query, StringComparison.CurrentCultureIgnoreCase)
                    || boundary.BoundaryValue.TreeRows.Any(row =>
                        row.DisplayPath.Contains(query, StringComparison.CurrentCultureIgnoreCase)),
                BridgeTriageEntry.Invalid invalid =>
                    invalid.InvalidCandidate.DisplayPath.Contains(query, StringComparison.CurrentCultureIgnoreCase),
                _ => false,
            };
    }

    private static string EntryTitle(BridgeTriageEntry entry) => entry switch
    {
        BridgeTriageEntry.Candidate candidate => DisplayTitle(candidate.Row),
        BridgeTriageEntry.Boundary boundary => boundary.BoundaryValue.Name,
        BridgeTriageEntry.Invalid invalid => invalid.InvalidCandidate.SourceFolderName,
        _ => string.Empty,
    };

    // The title a row leads with — the matched release's, or the folder name
    // when nothing matched. What sort and filter match against, because it's
    // what the row actually shows.
    internal static string DisplayTitle(BridgeTriageRow row) => row.Matched?.Title ?? row.FolderName;

    // Whether DisplayTitle fell through to the folder name — the rows that take
    // a folder glyph, so the title reads as a place on disk rather than a
    // release nobody has matched.
    internal static bool TitleIsFolderName(BridgeTriageRow row) => row.Matched is null;

    internal static BridgeTriageRow? Row(BridgeTriageQueue queue, string key) =>
        queue.Sections
            .SelectMany(section => section.Entries)
            .OfType<BridgeTriageEntry.Candidate>()
            .Select(candidate => candidate.Row)
            .FirstOrDefault(row => row.CandidateKey == key);

    internal static HashSet<string> CandidateKeys(BridgeTriageQueue queue) =>
        queue.Sections
            .SelectMany(section => section.Entries)
            .OfType<BridgeTriageEntry.Candidate>()
            .Select(candidate => candidate.Row.CandidateKey)
            .ToHashSet();

    internal static List<BridgeTriageRow> SelectableReadyRows(
        BridgeTriageQueue queue, string filterText, CandidateSortOrder sortOrder) =>
        Sections(queue, BridgeTriageTab.Ready, filterText, sortOrder)
            .SelectMany(section => section.Entries)
            .Select(entry => entry.Bridge)
            .OfType<BridgeTriageEntry.Candidate>()
            .Select(candidate => candidate.Row)
            .Where(row => row.Selectable)
            .ToList();

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
