using System;
using System.Collections.Generic;
using System.Linq;

namespace Bae.Windows;

// The storage dialog's four filter tabs. All lists every release; Unmanaged and
// Managed split on storage state; Uploading is driven solely by the release
// having pending outbox uploads.
public enum StorageTab { All, Unmanaged, Managed, Uploading }

// The five sortable columns. Storage (the sixth column) is inert, so it has no
// field here. SortDirection is reused from LibrarySort.cs.
public enum StorageSortField { AlbumTitle, ArtistNames, Format, FileCount, TotalSize }

// One release row as the model sees it, projected at the dialog edge from a
// BridgeStorageRow, the outbox snapshot, and the transfer overlay. Pure over
// primitives so tab membership, ordering, the footer, and the persisted sort are
// unit-tested apart from the WinUI dialog and the generated bridge types.
public sealed record StorageListRow(
    string ReleaseId,
    string AlbumTitle,
    string ArtistNames,
    string? Format,
    bool IsManaged,
    bool Pinned,
    long FileCount,
    long TotalSize,
    bool Uploading);

// The storage list's tab/sort/footer logic and its localized vocabulary. Client-
// side over the one full snapshot the dialog fetches: ordering an album title is
// a locale-aware compare (the locale never crosses the bridge), and keeping the
// whole matrix here makes it unit-testable.
public static class StorageListModel
{
    // Tab membership. Uploading reads only the outbox-driven flag, so an
    // uploading release lists under Uploading regardless of its storage state,
    // matching core's server-side Uploading filter.
    public static bool InTab(StorageListRow row, StorageTab tab) => tab switch
    {
        StorageTab.All => true,
        StorageTab.Unmanaged => !row.IsManaged,
        StorageTab.Managed => row.IsManaged,
        StorageTab.Uploading => row.Uploading,
        _ => throw new ArgumentOutOfRangeException(nameof(tab), tab, "Unknown storage tab"),
    };

    // The rows to display for a tab in sort order. Dedupes by ReleaseId (last
    // wins, matching the dialog's rowsById), never throws on any input, string
    // sorts are stable case-insensitive current-culture compares with a null
    // Format read as empty, numeric sorts on the raw longs, and ties keep
    // snapshot order.
    public static List<StorageListRow> Displayed(
        IReadOnlyList<StorageListRow> rows, StorageTab tab,
        StorageSortField field, SortDirection direction)
    {
        var inTab = DedupedInTab(rows, tab);
        IEnumerable<StorageListRow> ordered = field switch
        {
            StorageSortField.AlbumTitle => OrderString(inTab, row => row.AlbumTitle, direction),
            StorageSortField.ArtistNames => OrderString(inTab, row => row.ArtistNames, direction),
            StorageSortField.Format => OrderString(inTab, row => row.Format ?? string.Empty, direction),
            StorageSortField.FileCount => OrderNumber(inTab, row => row.FileCount, direction),
            StorageSortField.TotalSize => OrderNumber(inTab, row => row.TotalSize, direction),
            _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown storage sort field"),
        };
        return ordered.ToList();
    }

    // The footer over the displayed tab: the row count and the summed TotalSize.
    // Sums arithmetically over the deduped tab, so negative or duplicate rows
    // never throw.
    public static (int Count, long TotalSize) Footer(IReadOnlyList<StorageListRow> rows, StorageTab tab)
    {
        var inTab = DedupedInTab(rows, tab);
        return (inTab.Count, inTab.Sum(row => row.TotalSize));
    }

    // Header click: the active field flips direction; a new field selects it
    // ascending.
    public static (StorageSortField Field, SortDirection Direction) Toggle(
        StorageSortField currentField, SortDirection currentDirection, StorageSortField clicked) =>
        clicked == currentField
            ? (currentField, currentDirection == SortDirection.Ascending
                ? SortDirection.Descending
                : SortDirection.Ascending)
            : (clicked, SortDirection.Ascending);

    public static string TabLabelKey(StorageTab tab) => tab switch
    {
        StorageTab.All => "storage.tab.all",
        StorageTab.Unmanaged => "storage.tab.unmanaged",
        StorageTab.Managed => "storage.tab.managed",
        StorageTab.Uploading => "storage.tab.uploading",
        _ => throw new ArgumentOutOfRangeException(nameof(tab), tab, "Unknown storage tab"),
    };

    public static string ColumnLabelKey(StorageSortField field) => field switch
    {
        StorageSortField.AlbumTitle => "storage.column.album",
        StorageSortField.ArtistNames => "storage.column.artist",
        StorageSortField.Format => "storage.column.format",
        StorageSortField.FileCount => "storage.column.files",
        StorageSortField.TotalSize => "storage.column.size",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown storage sort field"),
    };

    // The inert sixth column's header label.
    public static string StorageColumnLabelKey => "storage.column.storage";

    // The persisted sort token, locale-free, "field:direction".
    public static string Serialize(StorageSortField field, SortDirection direction) =>
        $"{FieldToken(field)}:{DirectionToken(direction)}";

    // Parse a persisted token back to a field and direction. An absent, empty, or
    // unparseable token degrades to albumTitle ascending — a sort preference
    // degrades to the default rather than failing.
    public static (StorageSortField Field, SortDirection Direction) ParseSort(string? token)
    {
        var parts = token?.Split(':');
        if (parts is { Length: 2 }
            && TryParseField(parts[0], out var field)
            && TryParseDirection(parts[1], out var direction))
        {
            return (field, direction);
        }
        return (StorageSortField.AlbumTitle, SortDirection.Ascending);
    }

    private static List<StorageListRow> DedupedInTab(IReadOnlyList<StorageListRow> rows, StorageTab tab)
    {
        // Last write for a release id wins, keeping its first-seen position so
        // ties in the sort stay in snapshot order.
        var deduped = new List<StorageListRow>();
        var indexById = new Dictionary<string, int>();
        foreach (var row in rows)
        {
            if (indexById.TryGetValue(row.ReleaseId, out var existing))
            {
                deduped[existing] = row;
            }
            else
            {
                indexById[row.ReleaseId] = deduped.Count;
                deduped.Add(row);
            }
        }
        return deduped.Where(row => InTab(row, tab)).ToList();
    }

    private static IEnumerable<StorageListRow> OrderString(
        IEnumerable<StorageListRow> rows, Func<StorageListRow, string> key, SortDirection direction) =>
        direction == SortDirection.Ascending
            ? rows.OrderBy(key, StringComparer.CurrentCultureIgnoreCase)
            : rows.OrderByDescending(key, StringComparer.CurrentCultureIgnoreCase);

    private static IEnumerable<StorageListRow> OrderNumber(
        IEnumerable<StorageListRow> rows, Func<StorageListRow, long> key, SortDirection direction) =>
        direction == SortDirection.Ascending ? rows.OrderBy(key) : rows.OrderByDescending(key);

    private static string FieldToken(StorageSortField field) => field switch
    {
        StorageSortField.AlbumTitle => "albumTitle",
        StorageSortField.ArtistNames => "artistNames",
        StorageSortField.Format => "format",
        StorageSortField.FileCount => "fileCount",
        StorageSortField.TotalSize => "totalSize",
        _ => throw new ArgumentOutOfRangeException(nameof(field), field, "Unknown storage sort field"),
    };

    private static string DirectionToken(SortDirection direction) => direction switch
    {
        SortDirection.Ascending => "ascending",
        SortDirection.Descending => "descending",
        _ => throw new ArgumentOutOfRangeException(nameof(direction), direction, "Unknown sort direction"),
    };

    private static bool TryParseField(string? token, out StorageSortField field)
    {
        switch (token)
        {
            case "albumTitle": field = StorageSortField.AlbumTitle; return true;
            case "artistNames": field = StorageSortField.ArtistNames; return true;
            case "format": field = StorageSortField.Format; return true;
            case "fileCount": field = StorageSortField.FileCount; return true;
            case "totalSize": field = StorageSortField.TotalSize; return true;
            default: field = default; return false;
        }
    }

    private static bool TryParseDirection(string? token, out SortDirection direction)
    {
        switch (token)
        {
            case "ascending": direction = SortDirection.Ascending; return true;
            case "descending": direction = SortDirection.Descending; return true;
            default: direction = default; return false;
        }
    }
}

// The in-flight transfer overlay: release id → action token ("pin"/"unpin"/
// "manage"/"unmanage", the same wire tags NativeBae.TransferActionKey accepts).
// Apply overwrites; Clear of an unknown id is a no-op returning false; TokenFor
// returns null when the release is idle. Pure over primitives so the terminal-
// cleanup and no-op-clear behavior are unit-tested apart from the store shell.
public sealed class StorageTransferOverlay
{
    private readonly Dictionary<string, string> _tokens = new();

    public void Apply(string releaseId, string actionToken) => _tokens[releaseId] = actionToken;

    public bool Clear(string releaseId) => _tokens.Remove(releaseId);

    public string? TokenFor(string releaseId) => _tokens.TryGetValue(releaseId, out var token) ? token : null;

    public IReadOnlyCollection<string> ActiveReleaseIds => _tokens.Keys;
}
