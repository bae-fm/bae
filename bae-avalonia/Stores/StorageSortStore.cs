using System;
using System.IO;

namespace Bae.Desktop;

// The file-backed half of the storage-list sort persistence: the round-trip
// token lives in StorageListModel (unit-tested); this only moves that token to
// and from a blob under the app's local data directory, mirroring how macOS
// keeps the same preference in UserDefaults. A failed read or write degrades to
// the default sort rather than taking the app down — the sort is a preference,
// not durable state. The filter tab is not persisted; it resets to All on each
// open.
internal static class StorageSortStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "storage-sort.txt");

    // Load the saved field and direction, or the default when nothing is saved.
    public static (StorageSortField Field, SortDirection Direction) Load()
    {
        try
        {
            if (File.Exists(FilePath))
            {
                return StorageListModel.ParseSort(File.ReadAllText(FilePath).Trim());
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved storage sort.", exception);
        }

        return StorageListModel.ParseSort(null);
    }

    public static void Save(StorageSortField field, SortDirection direction)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, StorageListModel.Serialize(field, direction));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the storage sort.", exception);
        }
    }
}
