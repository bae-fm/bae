using System;
using System.Diagnostics;
using System.IO;

namespace Bae.Desktop;

// Reveal a file or folder in the OS file manager — the Windows/Linux analog of
// macOS's "Reveal in Finder", which the triage sidebar's row and folder menus
// offer next to the Skip/Remove actions. Windows Explorer can select a specific
// file within its parent; Linux has no portable "select" affordance across file
// managers, so xdg-open opens the containing folder instead. Best-effort: a
// failed launch is logged, not surfaced to the user — reveal is a convenience,
// not an operation with a result to report.
internal static class RevealInFileManager
{
    internal static void Reveal(string path)
    {
        try
        {
            if (OperatingSystem.IsWindows())
            {
                Process.Start(new ProcessStartInfo("explorer.exe", $"/select,\"{path}\"") { UseShellExecute = true });
            }
            else
            {
                var folder = Directory.Exists(path) ? path : Path.GetDirectoryName(path) ?? path;
                Process.Start(new ProcessStartInfo("xdg-open", $"\"{folder}\"") { UseShellExecute = true });
            }
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning($"Could not reveal '{path}' in the file manager.", exception);
        }
    }
}
