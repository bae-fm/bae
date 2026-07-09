using System.Collections.Generic;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The import candidate-list model: tab assignment (skipped wins over added,
// invalid always Skipped), per-tab counts, the four sort orders over core
// snapshot order, the Skipped tab's invalid-after-candidates split, and the
// persisted-token round-trip.
public sealed class ImportListModelTests
{
    private static ImportListRow Row(
        string key,
        string name = "name",
        bool skipped = false,
        bool isAdded = false,
        bool complete = false,
        bool invalid = false) =>
        new(key, name, skipped, isAdded, complete, invalid);

    [Fact]
    public void Tab_SkippedWinsOverCompleteAndAdded()
    {
        var row = Row("k", skipped: true, isAdded: true, complete: true);
        Assert.Equal(CandidateTab.Skipped, ImportListModel.Tab(row));
    }

    [Fact]
    public void Tab_InvalidAlwaysSkipped()
    {
        Assert.Equal(CandidateTab.Skipped, ImportListModel.Tab(Row("k", invalid: true)));
        // Invalid outranks an added match too.
        Assert.Equal(
            CandidateTab.Skipped,
            ImportListModel.Tab(Row("k", isAdded: true, complete: true, invalid: true)));
    }

    [Fact]
    public void Tab_CompleteIsAdded()
    {
        Assert.Equal(CandidateTab.Added, ImportListModel.Tab(Row("k", complete: true)));
    }

    [Fact]
    public void Tab_IsAddedIsAdded()
    {
        Assert.Equal(CandidateTab.Added, ImportListModel.Tab(Row("k", isAdded: true)));
    }

    [Fact]
    public void Tab_PlainIsNew()
    {
        Assert.Equal(CandidateTab.New, ImportListModel.Tab(Row("k")));
    }

    [Fact]
    public void Counts_TallyEachTab()
    {
        var rows = new List<ImportListRow>
        {
            Row("a"),
            Row("b"),
            Row("c", isAdded: true),
            Row("d", complete: true),
            Row("e", skipped: true),
            Row("f", invalid: true),
        };
        var counts = ImportListModel.Counts(rows);
        Assert.Equal(2, counts.New);
        Assert.Equal(2, counts.Added);
        Assert.Equal(2, counts.Skipped);
    }

    [Fact]
    public void DisplayedKeys_NewestIsInputOrder()
    {
        var rows = new List<ImportListRow> { Row("a"), Row("b"), Row("c") };
        Assert.Equal(
            new[] { "a", "b", "c" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.New, CandidateSortOrder.DateAddedNewest));
    }

    [Fact]
    public void DisplayedKeys_OldestIsExactReverse()
    {
        var rows = new List<ImportListRow> { Row("a"), Row("b"), Row("c") };
        Assert.Equal(
            new[] { "c", "b", "a" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.New, CandidateSortOrder.DateAddedOldest));
    }

    [Fact]
    public void DisplayedKeys_NameSortIsCaseInsensitive()
    {
        // Ordinal order would put "Bravo" (uppercase B) before "alpha" (lowercase a);
        // a case-insensitive compare orders alpha before bravo.
        var rows = new List<ImportListRow>
        {
            Row("bravo", name: "Bravo"),
            Row("alpha", name: "alpha"),
        };
        Assert.Equal(
            new[] { "alpha", "bravo" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.New, CandidateSortOrder.NameAZ));
        Assert.Equal(
            new[] { "bravo", "alpha" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.New, CandidateSortOrder.NameZA));
    }

    [Fact]
    public void DisplayedKeys_NameSortIsStableForEqualNames()
    {
        // Two rows whose names compare equal keep their input order in both directions.
        var rows = new List<ImportListRow>
        {
            Row("first", name: "same"),
            Row("second", name: "SAME"),
        };
        Assert.Equal(
            new[] { "first", "second" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.New, CandidateSortOrder.NameAZ));
        Assert.Equal(
            new[] { "first", "second" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.New, CandidateSortOrder.NameZA));
    }

    [Fact]
    public void DisplayedKeys_SkippedTabPutsInvalidAfterCandidates()
    {
        var rows = new List<ImportListRow>
        {
            Row("skip1", skipped: true),
            Row("invalid1", invalid: true),
            Row("skip2", skipped: true),
            Row("new1"),
        };
        // Newest keeps input order within each group: skipped first, invalid last.
        Assert.Equal(
            new[] { "skip1", "skip2", "invalid1" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.Skipped, CandidateSortOrder.DateAddedNewest));
        // Oldest reverses within each group, keeping non-invalid before invalid.
        Assert.Equal(
            new[] { "skip2", "skip1", "invalid1" },
            ImportListModel.DisplayedKeys(rows, CandidateTab.Skipped, CandidateSortOrder.DateAddedOldest));
    }

    [Fact]
    public void SerializeParse_RoundTripsEveryOrder()
    {
        foreach (var order in new[]
        {
            CandidateSortOrder.NameAZ,
            CandidateSortOrder.NameZA,
            CandidateSortOrder.DateAddedNewest,
            CandidateSortOrder.DateAddedOldest,
        })
        {
            Assert.Equal(order, ImportListModel.ParseSortOrder(ImportListModel.Serialize(order)));
        }
    }

    [Fact]
    public void ParseSortOrder_UnknownOrNullIsDefault()
    {
        Assert.Equal(CandidateSortOrder.DateAddedNewest, ImportListModel.ParseSortOrder(null));
        Assert.Equal(CandidateSortOrder.DateAddedNewest, ImportListModel.ParseSortOrder(""));
        Assert.Equal(CandidateSortOrder.DateAddedNewest, ImportListModel.ParseSortOrder("bogus"));
    }
}
