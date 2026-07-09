using System;
using System.IO;

namespace Bae.Windows;

// The file-backed half of the main window's bounds persistence: the on-disk
// shape, validation, and clamping math live in WindowBoundsModel (unit-tested);
// this only moves that JSON to and from a blob under the app's local data
// directory, mirroring how macOS lets the system restore the window frame. A
// failed read or write degrades to the system's default placement rather than
// taking the app down — the window bounds are a preference, not durable state.
internal static class WindowBoundsStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "window-bounds.json");

    // Load the saved bounds blob, or null when nothing is saved yet.
    public static string? Load()
    {
        try
        {
            if (File.Exists(FilePath))
            {
                return File.ReadAllText(FilePath);
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved window bounds.", exception);
        }

        return null;
    }

    public static void Save(string json)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, json);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the window bounds.", exception);
        }
    }
}
