using System;

namespace Bae.Desktop;

// The storage dialog's four filter tabs. All lists every release; Local and
// Cloud split on storage state; Uploading is driven solely by the release
// having pending outbox uploads. Tab membership is applied server-side (the
// dialog subscribes to a per-tab page through LibraryService), so this
// enum is purely the vocabulary the tab bar and the wire filter share.
public enum StorageTab { All, Local, Cloud, Uploading }

// The five sortable columns. Storage (the sixth column) is inert, so it has no
// field here. SortDirection is reused from LibrarySort.cs. Sorting is applied
// server-side (the storage subscription takes the field/direction), so this enum is
// the vocabulary the column headers and the wire sort share.
public enum StorageSortField { AlbumTitle, ArtistNames, Format, FileCount, TotalSize }

// The storage list's sort/persistence vocabulary. The tab-membership and
// row-ordering math this module once did client-side over a full library
// snapshot moved server-side when the dialog started paging instead of
// fetching everything at once (see StorageDialog); what's left here is the
// column-header click semantics and the persisted-sort round-trip, both
// independent of how many rows are loaded.
public static class StorageListModel
{
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
        StorageTab.Local => "storage.tab.local",
        StorageTab.Cloud => "storage.tab.cloud",
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
