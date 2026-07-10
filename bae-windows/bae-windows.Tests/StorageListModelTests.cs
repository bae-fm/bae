using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The storage list model: the column-header click semantics (Toggle), the
// persisted sort-token round-trip and its degrade-to-default, and the transfer
// overlay's apply/overwrite/clear/no-op behavior. Tab membership and row
// ordering are server-side now (NativeBae.StoragePage/StorageCount take the
// tab/sort directly) — see StorageDialog — so there is no client-side
// filter/sort matrix left to test here.
public sealed class StorageListModelTests
{
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
}
