using System;
using System.IO;

namespace Bae.Windows;

// The file-backed half of the sort persistence: the round-trip shape lives in
// SortCriteria<TField> (JSON, unit-tested); this only moves that JSON to and
// from a blob per mode under the app's local data directory, mirroring how
// macOS keeps the same arrays in UserDefaults. A failed read or write degrades
// to the default sort rather than taking the app down — the sort is a
// preference, not durable state.
internal static class LibrarySortStore
{
    private const string AlbumsFileName = "library-sort.json";
    private const string ComposersFileName = "library-sort-composers.json";
    private const string ArtistsFileName = "library-sort-artists.json";

    private static string DirectoryPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae");

    // Load the saved sort criteria for every mode into a fresh sort state, or the
    // default for a mode when nothing is saved yet.
    public static LibrarySort Load() => new(
        SortCriteria<AlbumSortField>.FromJson(LibrarySortVocab.Album, Read(AlbumsFileName)),
        SortCriteria<ComposerSortField>.FromJson(LibrarySortVocab.Composer, Read(ComposersFileName)),
        SortCriteria<ArtistSortField>.FromJson(LibrarySortVocab.Artist, Read(ArtistsFileName)));

    public static void SaveAlbums(SortCriteria<AlbumSortField> albums) => Write(AlbumsFileName, albums.ToJson());

    public static void SaveComposers(SortCriteria<ComposerSortField> composers) => Write(ComposersFileName, composers.ToJson());

    public static void SaveArtists(SortCriteria<ArtistSortField> artists) => Write(ArtistsFileName, artists.ToJson());

    private static string? Read(string fileName)
    {
        try
        {
            var path = Path.Combine(DirectoryPath, fileName);
            return File.Exists(path) ? File.ReadAllText(path) : null;
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved library sort.", exception);
            return null;
        }
    }

    private static void Write(string fileName, string json)
    {
        try
        {
            Directory.CreateDirectory(DirectoryPath);
            File.WriteAllText(Path.Combine(DirectoryPath, fileName), json);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the library sort.", exception);
        }
    }
}
