using System;
using System.IO;

namespace Bae.Windows;

// The file-backed choice of which mode the now-playing bar's leading time label
// shows: elapsed, or a minus-prefixed remaining countdown. The bare token lives
// under the app's local data directory, mirroring how macOS keeps the same
// choice in UserDefaults. A failed read or write degrades to elapsed rather than
// taking the app down — the label mode is a preference, not durable state.
internal static class TimeLabelStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "time-label.txt");

    // Load the saved label mode, or elapsed when nothing is saved yet.
    public static bool Load()
    {
        string? token = null;
        try
        {
            if (File.Exists(FilePath))
            {
                token = File.ReadAllText(FilePath);
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved time-label mode.", exception);
        }

        return PlaybackPositionModel.ShowRemainingFromToken(token);
    }

    public static void Save(bool showRemaining)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, PlaybackPositionModel.TimeLabelToken(showRemaining));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the time-label mode.", exception);
        }
    }
}
