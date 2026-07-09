using System;
using System.Collections.Generic;
using System.Linq;

namespace Bae.Windows;

// Which of the import dialog's three tabs a candidate lists under.
public enum CandidateTab
{
    New,
    Added,
    Skipped,
}

// The candidate-list sort order the dialog offers. The enum order is the sort
// ComboBox order (a selected index maps straight to the value); the persisted
// token is stable across platforms (the macOS catalog persists the same four).
public enum CandidateSortOrder
{
    NameAZ,
    NameZA,
    DateAddedNewest,
    DateAddedOldest,
}

// One candidate as the list model sees it: identity, display name, and the state
// bits that decide its tab. Pure over primitives so tab assignment, counting, and
// ordering are unit-tested apart from the WinUI dialog and the generated bridge
// types.
public sealed record ImportListRow(
    string Key,
    string Name,
    bool Skipped,
    bool IsAdded,
    bool ImportComplete,
    bool Invalid);

public static class ImportListModel
{
    // Skipped wins over Added (a skipped candidate stays under Skipped even when it
    // was also imported); an invalid folder always lists under Skipped; a candidate
    // that completed an import or matches an already-imported folder is Added;
    // everything else is New.
    public static CandidateTab Tab(ImportListRow row)
    {
        if (row.Invalid || row.Skipped)
        {
            return CandidateTab.Skipped;
        }
        if (row.ImportComplete || row.IsAdded)
        {
            return CandidateTab.Added;
        }
        return CandidateTab.New;
    }

    public static (int New, int Added, int Skipped) Counts(IEnumerable<ImportListRow> rows)
    {
        var counts = (New: 0, Added: 0, Skipped: 0);
        foreach (var row in rows)
        {
            switch (Tab(row))
            {
                case CandidateTab.New:
                    counts.New++;
                    break;
                case CandidateTab.Added:
                    counts.Added++;
                    break;
                case CandidateTab.Skipped:
                    counts.Skipped++;
                    break;
            }
        }
        return counts;
    }

    // The keys to display for `tab`, ordered by `order`. Input order is the core
    // snapshot order (watched-folder add order, then path), which is what "date
    // added" means: newest keeps it, oldest reverses it; the name sorts are stable
    // case-insensitive current-culture compares. Within the Skipped tab, the
    // non-invalid rows come before the invalid ones, each group ordered
    // independently.
    public static List<string> DisplayedKeys(
        IReadOnlyList<ImportListRow> rows, CandidateTab tab, CandidateSortOrder order)
    {
        var inTab = rows.Where(row => Tab(row) == tab).ToList();
        if (tab == CandidateTab.Skipped)
        {
            var valid = Order(inTab.Where(row => !row.Invalid), order);
            var invalid = Order(inTab.Where(row => row.Invalid), order);
            return valid.Concat(invalid).ToList();
        }
        return Order(inTab, order).ToList();
    }

    private static IEnumerable<string> Order(IEnumerable<ImportListRow> rows, CandidateSortOrder order) =>
        order switch
        {
            CandidateSortOrder.NameAZ => rows
                .OrderBy(row => row.Name, StringComparer.CurrentCultureIgnoreCase)
                .Select(row => row.Key),
            CandidateSortOrder.NameZA => rows
                .OrderByDescending(row => row.Name, StringComparer.CurrentCultureIgnoreCase)
                .Select(row => row.Key),
            CandidateSortOrder.DateAddedNewest => rows.Select(row => row.Key),
            CandidateSortOrder.DateAddedOldest => rows.Select(row => row.Key).Reverse(),
            _ => throw new ArgumentOutOfRangeException(nameof(order), order, "Unknown sort order"),
        };

    // Round-trip tokens for the persisted sort preference. The tokens match the
    // values the macOS catalog persists for the same preference.
    public static string Serialize(CandidateSortOrder order) => order switch
    {
        CandidateSortOrder.NameAZ => "nameAZ",
        CandidateSortOrder.NameZA => "nameZA",
        CandidateSortOrder.DateAddedNewest => "dateAddedNewest",
        CandidateSortOrder.DateAddedOldest => "dateAddedOldest",
        _ => throw new ArgumentOutOfRangeException(nameof(order), order, "Unknown sort order"),
    };

    // An unknown or absent token falls back to the default order — a sort
    // preference degrades to the default rather than failing.
    public static CandidateSortOrder ParseSortOrder(string? token) => token switch
    {
        "nameAZ" => CandidateSortOrder.NameAZ,
        "nameZA" => CandidateSortOrder.NameZA,
        "dateAddedNewest" => CandidateSortOrder.DateAddedNewest,
        "dateAddedOldest" => CandidateSortOrder.DateAddedOldest,
        _ => CandidateSortOrder.DateAddedNewest,
    };
}
