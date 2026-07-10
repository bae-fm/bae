using System;
using System.IO;

namespace Bae.Windows;

// The file-backed choice of whether quitting the app performs the graceful
// shutdown that saves the current track, position, queue, and volume for the
// next launch to restore. The bare token lives under the app's local data
// directory, mirroring how macOS keeps the same choice in UserDefaults under
// the persistPlayback key. A failed read or write degrades to off rather than
// taking the app down — the preference is a preference, not durable state.
internal static class PersistPlaybackStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "persist-playback.txt");

    // Load the saved preference, or off when nothing is saved yet.
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
            BaeDiagnostics.Logger.Warning("Could not read the saved restore-on-launch preference.", exception);
        }

        return PersistPlaybackModel.PersistFromToken(token);
    }

    public static void Save(bool persist)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, PersistPlaybackModel.Token(persist));
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the restore-on-launch preference.", exception);
        }
    }
}
