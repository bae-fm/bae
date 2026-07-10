using System.Collections.Generic;
using System.Linq;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The storage list model: tab membership over a mixed snapshot, the five sortable
// fields in both directions (case-insensitive strings, null format as empty, raw
// numeric, stable ties), the persisted sort-token round-trip and its degrade-to-
// default, and the transfer overlay's apply/overwrite/clear/no-op behavior plus
// terminal-event cleanup.
public sealed class StorageListModelTests
{
    private static StorageListRow Row(
        string id,
        string album = "album",
        string artist = "artist",
        string? format = "FLAC",
        bool managed = false,
        bool pinned = false,
        long fileCount = 1,
        long totalSize = 1,
        bool uploading = false) =>
        new(id, album, artist, format, managed, pinned, fileCount, totalSize, uploading);

    // ── Tab membership ───────────────────────────────────────────────────────

    [Fact]
    public void InTab_MixedSnapshotLandsEachRowInTheRightTabs()
    {
        var unmanaged = Row("a", managed: false);
        var managed = Row("b", managed: true);
        var managedPinned = Row("c", managed: true, pinned: true);
        var uploadingUnmanaged = Row("d", managed: false, uploading: true);

        Assert.True(StorageListModel.InTab(unmanaged, StorageTab.Unmanaged));
        Assert.False(StorageListModel.InTab(unmanaged, StorageTab.Managed));

        Assert.True(StorageListModel.InTab(managed, StorageTab.Managed));
        Assert.False(StorageListModel.InTab(managed, StorageTab.Unmanaged));

        Assert.True(StorageListModel.InTab(managedPinned, StorageTab.Managed));

        // Uploading is driven solely by the flag, regardless of storage state.
        Assert.True(StorageListModel.InTab(uploadingUnmanaged, StorageTab.Uploading));
        Assert.True(StorageListModel.InTab(uploadingUnmanaged, StorageTab.Unmanaged));
        Assert.False(StorageListModel.InTab(managed, StorageTab.Uploading));

        // All contains everything.
        foreach (var row in new[] { unmanaged, managed, managedPinned, uploadingUnmanaged })
        {
            Assert.True(StorageListModel.InTab(row, StorageTab.All));
        }
    }

    [Fact]
    public void Displayed_AllTabContainsEveryRow()
    {
        var rows = new List<StorageListRow>
        {
            Row("a", managed: false),
            Row("b", managed: true),
            Row("c", managed: true, uploading: true),
        };
        var displayed = StorageListModel.Displayed(
            rows, StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Ascending);
        Assert.Equal(new[] { "a", "b", "c" }, displayed.Select(row => row.ReleaseId));
    }

    // ── Sort correctness ─────────────────────────────────────────────────────

    [Fact]
    public void Displayed_AlbumTitleSortIsCaseInsensitiveBothDirections()
    {
        var rows = new List<StorageListRow>
        {
            Row("bravo", album: "Bravo"),
            Row("alpha", album: "alpha"),
        };
        Assert.Equal(
            new[] { "alpha", "bravo" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
        Assert.Equal(
            new[] { "bravo", "alpha" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Descending)
                .Select(row => row.ReleaseId));
    }

    [Fact]
    public void Displayed_ArtistNamesSortOrders()
    {
        var rows = new List<StorageListRow>
        {
            Row("y", artist: "Yankee"),
            Row("x", artist: "xray"),
        };
        Assert.Equal(
            new[] { "x", "y" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.ArtistNames, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
    }

    [Fact]
    public void Displayed_NullFormatSortsAsEmpty()
    {
        var rows = new List<StorageListRow>
        {
            Row("has", format: "FLAC"),
            Row("none", format: null),
        };
        // Ascending: empty (null) sorts before "FLAC".
        Assert.Equal(
            new[] { "none", "has" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.Format, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
    }

    [Fact]
    public void Displayed_FileCountAndTotalSizeSortNumerically()
    {
        var rows = new List<StorageListRow>
        {
            Row("big", fileCount: 100, totalSize: 9),
            Row("small", fileCount: 9, totalSize: 100),
        };
        // A raw numeric compare, not lexicographic (9 < 100).
        Assert.Equal(
            new[] { "small", "big" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.FileCount, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
        Assert.Equal(
            new[] { "big", "small" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.TotalSize, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
    }

    [Fact]
    public void Displayed_TiesPreserveSnapshotOrder()
    {
        var rows = new List<StorageListRow>
        {
            Row("first", album: "same"),
            Row("second", album: "SAME"),
            Row("third", album: "same"),
        };
        // Equal titles keep input order in both directions (stable).
        Assert.Equal(
            new[] { "first", "second", "third" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
        Assert.Equal(
            new[] { "first", "second", "third" },
            StorageListModel.Displayed(rows, StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Descending)
                .Select(row => row.ReleaseId));
    }

    [Fact]
    public void Toggle_FlipsActiveFieldAndResetsNewField()
    {
        // Same field flips direction.
        Assert.Equal(
            (StorageSortField.AlbumTitle, SortDirection.Descending),
            StorageListModel.Toggle(StorageSortField.AlbumTitle, SortDirection.Ascending, StorageSortField.AlbumTitle));
        Assert.Equal(
            (StorageSortField.AlbumTitle, SortDirection.Ascending),
            StorageListModel.Toggle(StorageSortField.AlbumTitle, SortDirection.Descending, StorageSortField.AlbumTitle));
        // A new field selects it ascending, whatever the prior direction was.
        Assert.Equal(
            (StorageSortField.TotalSize, SortDirection.Ascending),
            StorageListModel.Toggle(StorageSortField.AlbumTitle, SortDirection.Descending, StorageSortField.TotalSize));
    }

    // ── Persistence round-trip ───────────────────────────────────────────────

    [Fact]
    public void SerializeParse_RoundTripsEveryFieldAndDirection()
    {
        foreach (var field in new[]
        {
            StorageSortField.AlbumTitle,
            StorageSortField.ArtistNames,
            StorageSortField.Format,
            StorageSortField.FileCount,
            StorageSortField.TotalSize,
        })
        {
            foreach (var direction in new[] { SortDirection.Ascending, SortDirection.Descending })
            {
                Assert.Equal(
                    (field, direction),
                    StorageListModel.ParseSort(StorageListModel.Serialize(field, direction)));
            }
        }
    }

    [Fact]
    public void ParseSort_GarbageEmptyOrNullDegradesToDefault()
    {
        var expected = (StorageSortField.AlbumTitle, SortDirection.Ascending);
        Assert.Equal(expected, StorageListModel.ParseSort(null));
        Assert.Equal(expected, StorageListModel.ParseSort(""));
        Assert.Equal(expected, StorageListModel.ParseSort("bogus"));
        Assert.Equal(expected, StorageListModel.ParseSort("albumTitle"));
        Assert.Equal(expected, StorageListModel.ParseSort("albumTitle:sideways"));
        Assert.Equal(expected, StorageListModel.ParseSort("nope:ascending"));
    }

    // ── Progress overlay ─────────────────────────────────────────────────────

    [Fact]
    public void Overlay_ApplyTokenForOverwriteAndClear()
    {
        var overlay = new StorageTransferOverlay();
        overlay.Apply("r1", "pin");
        Assert.Equal("pin", overlay.TokenFor("r1"));

        // A second apply for the same id overwrites.
        overlay.Apply("r1", "unpin");
        Assert.Equal("unpin", overlay.TokenFor("r1"));

        Assert.Contains("r1", overlay.ActiveReleaseIds);

        // Clear removes and returns true.
        Assert.True(overlay.Clear("r1"));
        Assert.Null(overlay.TokenFor("r1"));
        Assert.DoesNotContain("r1", overlay.ActiveReleaseIds);
    }

    [Fact]
    public void Overlay_ClearOfUnknownIdIsNoOpReturningFalse()
    {
        var overlay = new StorageTransferOverlay();
        // A release id never applied — including one absent from every row of the
        // current filter — clears to false and throws nothing.
        Assert.False(overlay.Clear("never-applied"));
        Assert.Null(overlay.TokenFor("never-applied"));
        Assert.Empty(overlay.ActiveReleaseIds);
    }

    [Fact]
    public void Overlay_TerminalEventCleanupLeavesDisplayedUnaffected()
    {
        var rows = new List<StorageListRow>
        {
            Row("visible", managed: true),
            Row("filtered", managed: false),
        };

        // A row filtered out of the Managed tab gets an in-flight transfer, then
        // the ended path clears it. Displayed output stays constant throughout.
        var overlay = new StorageTransferOverlay();
        var managedBefore = StorageListModel.Displayed(
            rows, StorageTab.Managed, StorageSortField.AlbumTitle, SortDirection.Ascending);

        overlay.Apply("filtered", "manage");
        Assert.True(overlay.Clear("filtered"));
        Assert.Empty(overlay.ActiveReleaseIds);

        foreach (var tab in new[] { StorageTab.All, StorageTab.Unmanaged, StorageTab.Managed, StorageTab.Uploading })
        {
            var before = StorageListModel.Displayed(rows, tab, StorageSortField.AlbumTitle, SortDirection.Ascending);
            var after = StorageListModel.Displayed(rows, tab, StorageSortField.AlbumTitle, SortDirection.Ascending);
            Assert.Equal(before.Select(row => row.ReleaseId), after.Select(row => row.ReleaseId));
        }
        Assert.Equal(
            managedBefore.Select(row => row.ReleaseId),
            StorageListModel.Displayed(rows, StorageTab.Managed, StorageSortField.AlbumTitle, SortDirection.Ascending)
                .Select(row => row.ReleaseId));
    }

    // ── Garbage snapshot handling ────────────────────────────────────────────

    [Fact]
    public void Displayed_DuplicateReleaseIdsLastWins()
    {
        var rows = new List<StorageListRow>
        {
            Row("dup", album: "first", totalSize: 10),
            Row("dup", album: "second", totalSize: 40),
        };
        var displayed = StorageListModel.Displayed(
            rows, StorageTab.All, StorageSortField.AlbumTitle, SortDirection.Ascending);
        Assert.Single(displayed);
        Assert.Equal("second", displayed[0].AlbumTitle);

        var footer = StorageListModel.Footer(rows, StorageTab.All);
        Assert.Equal(1, footer.Count);
        Assert.Equal(40, footer.TotalSize);
    }

    [Fact]
    public void Displayed_HandlesBlankFieldsAndNegativeNumbers()
    {
        var rows = new List<StorageListRow>
        {
            Row("blank", album: "  ", artist: "", fileCount: -5, totalSize: -20),
            Row("plain", album: "plain", fileCount: 3, totalSize: 30),
        };
        // Negative numbers sum arithmetically, no throw.
        var footer = StorageListModel.Footer(rows, StorageTab.All);
        Assert.Equal(2, footer.Count);
        Assert.Equal(10, footer.TotalSize);

        // Sorting over blank titles/negatives doesn't throw.
        var displayed = StorageListModel.Displayed(
            rows, StorageTab.All, StorageSortField.FileCount, SortDirection.Ascending);
        Assert.Equal(new[] { "blank", "plain" }, displayed.Select(row => row.ReleaseId));
    }

    [Fact]
    public void Displayed_EmptySnapshotIsEmptyEveryTabFooterZero()
    {
        var rows = new List<StorageListRow>();
        foreach (var tab in new[] { StorageTab.All, StorageTab.Unmanaged, StorageTab.Managed, StorageTab.Uploading })
        {
            Assert.Empty(StorageListModel.Displayed(rows, tab, StorageSortField.AlbumTitle, SortDirection.Ascending));
            Assert.Equal((0, 0L), StorageListModel.Footer(rows, tab));
        }
    }
}
