using System;
using System.IO;

namespace Bae.Windows;

// The last output folder a release export wrote to, remembered per device so the
// folder picker can seed itself with it on the next export. The destination is a
// per-run choice, never config; this is only a UI convenience memory (the Windows
// sibling of macOS's lastExportFolder), so a failed read or write degrades to "no
// folder" rather than taking the app down.
internal static class OutputFolderStore
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "output-folder.txt");

    // The remembered folder, or null when nothing is saved yet or the file is
    // blank/unreadable.
    public static string? Load()
    {
        try
        {
            if (File.Exists(FilePath))
            {
                var path = File.ReadAllText(FilePath).Trim();
                return string.IsNullOrEmpty(path) ? null : path;
            }
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not read the saved export folder.", exception);
        }

        return null;
    }

    // Remember the folder a release export just wrote into.
    public static void Save(string dir)
    {
        try
        {
            var path = FilePath;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            File.WriteAllText(path, dir);
        }
        catch (Exception exception) when (exception is IOException or UnauthorizedAccessException)
        {
            BaeDiagnostics.Logger.Warning("Could not save the export folder.", exception);
        }
    }
}
