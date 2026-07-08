using System;
using System.IO;

namespace Bae.Windows;

// The file-backed half of the album sort persistence: the round-trip shape lives
// in AlbumSortCriteria (JSON, unit-tested); this only moves that JSON to and from
// a blob under the app's local data directory, mirroring how macOS keeps the same
// array in UserDefaults. A failed read or write degrades to the default sort
// rather than taking the app down — the sort is a preference, not durable state.
internal static class LibrarySortStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "library-sort.json");

    // Load the saved album sort criteria into a fresh sort state, or the default
    // when nothing is saved yet.
    public static LibrarySort Load()
    {
        string? json = null;
        try
        {
            if (File.Exists(FilePath))
            {
                json = File.ReadAllText(FilePath);
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved library sort.", exception);
        }

        return new LibrarySort(AlbumSortCriteria.FromJson(json));
    }

    public static void SaveAlbums(AlbumSortCriteria albums)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, albums.ToJson());
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the library sort.", exception);
        }
    }
}
