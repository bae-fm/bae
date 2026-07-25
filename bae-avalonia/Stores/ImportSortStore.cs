using System;
using System.IO;

namespace Bae.Desktop;

// The file-backed half of the import candidate-sort persistence: the round-trip
// token lives in ImportListModel (unit-tested); this only moves that token to and
// from a blob under the app's local data directory, mirroring how macOS keeps the
// same preference in UserDefaults. A failed read or write degrades to the default
// sort rather than taking the app down — the sort is a preference, not durable
// state.
internal static class ImportSortStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "import-sort.txt");

    // Load the saved sort order, or the default when nothing is saved yet.
    public static CandidateSortOrder Load()
    {
        try
        {
            if (File.Exists(FilePath))
            {
                return ImportListModel.ParseSortOrder(File.ReadAllText(FilePath).Trim());
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved import sort.", exception);
        }

        return ImportListModel.ParseSortOrder(null);
    }

    public static void Save(CandidateSortOrder order)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, ImportListModel.Serialize(order));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the import sort.", exception);
        }
    }
}
