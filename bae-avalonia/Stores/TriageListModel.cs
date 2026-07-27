using System;
using System.Collections.Generic;
using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The sidebar row list's sort order — the only thing left for the UI to decide
// once core has placed every row into its tab and, under Needs you, its group.
// Only name order survives the triage redesign: a BridgeTriageRow carries no
// discovery timestamp (BridgeTriageQueue.Rows is ordered by watched folder then
// candidate key, not by when a candidate was found), so a "date added" option
// would silently degrade into an alias for name order. Better to drop it than
// keep a control that lies about what it does.
internal enum CandidateSortOrder
{
    NameAZ,
    NameZA,
}

// One row under the Skipped tab: a manually-skipped candidate, or an invalid
// folder (looked like a release but failed validation). Both come off
// BridgeTriageQueue — core already decided both belong here — this only pairs
// them into one list a view can iterate.
internal abstract record SkippedRow
{
    internal sealed record Candidate(BridgeTriageRow Row) : SkippedRow;

    internal sealed record Invalid(BridgeInvalidCandidate InvalidCandidate) : SkippedRow;

    // What the row sorts and filters by: the candidate's display title, or the
    // invalid folder's source name.
    internal string SortTitle => this switch
    {
        Candidate c => TriageListModel.DisplayTitle(c.Row),
        Invalid i => i.InvalidCandidate.SourceFolderName,
        _ => string.Empty,
    };

    // Extra filter text beyond SortTitle: the folder path, for a query that
    // names disk structure rather than a title.
    internal string FilterPath => this switch
    {
        Candidate c => c.Row.CandidateKey,
        Invalid i => i.InvalidCandidate.FolderPath,
        _ => string.Empty,
    };
}

// The sidebar's row list, grouping, filtering, and sort order, read off
// BridgeTriageQueue — core's projection. This computes nothing about which tab
// or Needs-you group a row belongs to (BridgeTriageRow.Placement already says
// that); it only filters, groups, and orders what core already placed.
internal static class TriageListModel
{
    // The title a row leads with — the matched release's, or the folder name
    // when nothing matched. What sort and filter match against, because it's
    // what the row actually shows.
    internal static string DisplayTitle(BridgeTriageRow row) => row.Matched?.Title ?? row.FolderName;

    // The rows in `tab`, filtered by `filterText` and ordered by `sortOrder`.
    // Tab membership is `Placement`'s, computed once in core — this only
    // filters and orders what core already placed. Used for Ready, Done, and
    // Skipped's candidate half; Needs you uses NeedsYouGroups instead, since it
    // groups by question rather than rendering one flat list.
    internal static List<BridgeTriageRow> Rows(
        BridgeTriageQueue queue, BridgeTriageTab tab, string filterText, CandidateSortOrder sortOrder)
    {
        var scoped = queue.Rows.Where(row => BaeBridgeMethods.BridgeTriageTab(row.Placement) == tab);
        return Order(Filtered(scoped, filterText), sortOrder, DisplayTitle).ToList();
    }

    // Needs-you rows grouped by the question they ask, in the order core
    // declares (BridgeNeedsYouGroupsInOrder) — the one place that ordering is
    // stated. A group with nothing left in it (everything filtered out, or
    // everything answered) drops out rather than rendering an empty header.
    internal static List<(BridgeNeedsYouGroup Group, List<BridgeTriageRow> Rows)> NeedsYouGroups(
        BridgeTriageQueue queue, string filterText, CandidateSortOrder sortOrder)
    {
        var needsYou = Filtered(
            queue.Rows.Where(row => BaeBridgeMethods.BridgeTriageTab(row.Placement) == BridgeTriageTab.NeedsYou),
            filterText).ToList();

        var result = new List<(BridgeNeedsYouGroup, List<BridgeTriageRow>)>();
        foreach (var group in BaeBridgeMethods.BridgeNeedsYouGroupsInOrder())
        {
            var rows = Order(
                needsYou.Where(row =>
                    row.Placement is BridgeTriagePlacement.NeedsYou placement && placement.Group == group),
                sortOrder,
                DisplayTitle).ToList();
            if (rows.Count > 0)
            {
                result.Add((group, rows));
            }
        }
        return result;
    }

    // The Skipped tab: manually-skipped candidates and invalid folders as one
    // filtered, ordered list — the tab shows both under a single header, so
    // there is nothing to group by within it.
    internal static List<SkippedRow> SkippedRows(
        BridgeTriageQueue queue, string filterText, CandidateSortOrder sortOrder)
    {
        var candidateRows = queue.Rows
            .Where(row => BaeBridgeMethods.BridgeTriageTab(row.Placement) == BridgeTriageTab.Skipped)
            .Select(row => (SkippedRow)new SkippedRow.Candidate(row));
        var invalidRows = queue.Invalid.Select(candidate => (SkippedRow)new SkippedRow.Invalid(candidate));
        var all = candidateRows.Concat(invalidRows);

        var query = filterText.Trim().ToLowerInvariant();
        var matching = query.Length == 0
            ? all
            : all.Where(row =>
                row.SortTitle.ToLowerInvariant().Contains(query)
                || row.FilterPath.ToLowerInvariant().Contains(query));

        return Order(matching, sortOrder, row => row.SortTitle).ToList();
    }

    // Order `rows` by `sortOrder`, keying the name comparison off `title`. One
    // dispatch for triage rows (by display title) and skipped rows (by title or
    // invalid-folder name), so a new CandidateSortOrder case is handled in one
    // place. LINQ's OrderBy/OrderByDescending are stable, so two rows whose
    // titles compare equal keep their input order.
    private static IEnumerable<T> Order<T>(
        IEnumerable<T> rows, CandidateSortOrder sortOrder, Func<T, string> title) =>
        sortOrder switch
        {
            CandidateSortOrder.NameAZ => rows.OrderBy(title, StringComparer.CurrentCultureIgnoreCase),
            CandidateSortOrder.NameZA => rows.OrderByDescending(title, StringComparer.CurrentCultureIgnoreCase),
            _ => throw new ArgumentOutOfRangeException(nameof(sortOrder), sortOrder, "Unknown sort order"),
        };

    // Filter `rows` by `filterText` against the display title, the folder name,
    // and the candidate key (folder path) — a query can name either what
    // matched or the folder on disk.
    private static IEnumerable<BridgeTriageRow> Filtered(IEnumerable<BridgeTriageRow> rows, string filterText)
    {
        var query = filterText.Trim().ToLowerInvariant();
        if (query.Length == 0)
        {
            return rows;
        }
        return rows.Where(row =>
            DisplayTitle(row).ToLowerInvariant().Contains(query)
            || row.FolderName.ToLowerInvariant().Contains(query)
            || row.CandidateKey.ToLowerInvariant().Contains(query));
    }

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
