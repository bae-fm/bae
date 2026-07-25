using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.Versioning;
using System.Text;
using Microsoft.Win32;

namespace Bae.Desktop;

/// <summary>
/// Declares this app to the OS as the handler for <c>bae://</c> links and for
/// folders — the desktop equivalent of macOS's <c>CFBundleURLTypes</c> /
/// <c>CFBundleDocumentTypes</c>, written by hand because neither desktop package
/// carries a manifest the OS reads those from: the Windows build is unpackaged
/// and unsigned, and the Linux build is an <c>.AppImage</c>, a single file the
/// desktop only learns anything about from what the app writes itself.
///
/// Both halves are per-user and need no elevation or signing: Windows writes
/// under <c>HKCU\Software\Classes</c>, Linux writes one desktop entry under the
/// user's own data directory.
/// </summary>
internal static class ProtocolRegistration
{
    private const string ProtocolKeyPath = @"Software\Classes\bae";
    private const string FolderVerbKeyPath = @"Software\Classes\Directory\shell\bae";

    // Long enough for a desktop-database rebuild over one directory, short enough
    // that a helper that never returns can't hold launch open behind it.
    private const int DesktopDatabaseTimeoutMs = 5000;

    // The specification asks for UTF-8; a byte-order mark ahead of the group
    // header is not part of that, and parsers that read the header literally
    // choke on one.
    private static readonly UTF8Encoding EntryEncoding = new(encoderShouldEmitUTF8Identifier: false);

    /// <summary>
    /// Write the registration, replacing whatever is already there. Called on
    /// every launch (gated by the caller on this being a Velopack install), so the
    /// command always names where the app is now and the Windows folder verb's
    /// label re-resolves after a locale change. A write failure (an unreadable
    /// hive, a data directory that can't be written, a policy in the way) is
    /// logged and leaves whatever registration already existed in place.
    /// </summary>
    internal static void Register()
    {
        if (OperatingSystem.IsWindows())
        {
            RegisterShellKeys();
        }
        else if (OperatingSystem.IsLinux())
        {
            RegisterDesktopEntry();
        }
    }

    /// <summary>
    /// Remove the Windows registration. Run from the Velopack pre-uninstall
    /// callback, ahead of any UI or resource initialization — it needs no
    /// localized string, so it is safe there.
    ///
    /// There is no Linux counterpart to run it from: Velopack triggers an
    /// uninstall hook on Windows only, and an <c>.AppImage</c> is removed by
    /// whoever put it there, with nothing of ours running. A desktop entry so
    /// left behind names an executable that is gone.
    /// </summary>
    internal static void Unregister()
    {
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        try
        {
            Registry.CurrentUser.DeleteSubKeyTree(ProtocolKeyPath, throwOnMissingSubKey: false);
            Registry.CurrentUser.DeleteSubKeyTree(FolderVerbKeyPath, throwOnMissingSubKey: false);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning(
                "Could not remove the bae:// protocol / folder verb registration.", exception);
        }
    }

    // ── Windows: the per-user class registrations ────────────────────────────

    [SupportedOSPlatform("windows")]
    private static void RegisterShellKeys()
    {
        var exePath = Environment.ProcessPath;
        if (string.IsNullOrEmpty(exePath))
        {
            BaeDiagnostics.Logger.Warning(
                "Could not resolve the process path; skipping bae:// protocol registration.");
            return;
        }

        var quotedExe = $"\"{exePath}\"";
        var command = $"{quotedExe} \"%1\"";
        var icon = $"{quotedExe},0";

        try
        {
            using var protocolKey = Registry.CurrentUser.CreateSubKey(ProtocolKeyPath);
            protocolKey.SetValue(null, "URL:bae");
            protocolKey.SetValue("URL Protocol", string.Empty);
            using (var defaultIconKey = protocolKey.CreateSubKey("DefaultIcon"))
            {
                defaultIconKey.SetValue(null, icon);
            }
            using (var commandKey = protocolKey.CreateSubKey(@"shell\open\command"))
            {
                commandKey.SetValue(null, command);
            }

            using var folderVerbKey = Registry.CurrentUser.CreateSubKey(FolderVerbKeyPath);
            folderVerbKey.SetValue(null, Loc.Chrome("shell.open_with_folder", "name", App.Edition));
            folderVerbKey.SetValue("Icon", icon);
            using (var folderCommandKey = folderVerbKey.CreateSubKey("command"))
            {
                folderCommandKey.SetValue(null, command);
            }
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not register the bae:// protocol / folder verb.", exception);
        }
    }

    // ── Linux: the desktop entry ─────────────────────────────────────────────

    // One file naming the app's command and the two things it handles.
    private static void RegisterDesktopEntry()
    {
        // The only durable path to the app: the executable this process runs from
        // sits inside the .AppImage's mount, which is unmounted the moment the
        // process exits, so an entry written from it would be dead before anything
        // read it. The AppImage runtime exports the image's own path here — and the
        // Velopack install this registration is gated on is itself detected by that
        // same variable naming a file that exists, so inside the gate it is always
        // set. Absent, there is no path worth writing.
        var appImagePath = Environment.GetEnvironmentVariable("APPIMAGE");
        if (string.IsNullOrEmpty(appImagePath))
        {
            BaeDiagnostics.Logger.Warning(
                "Could not resolve the .AppImage path ($APPIMAGE); skipping the desktop entry.");
            return;
        }

        var directory = ApplicationsDirectory();
        // Named for the edition, so bae and baeium own separate entries — and that
        // basename is what the MPRIS surface names as its DesktopEntry, which is
        // this file.
        var entryPath = Path.Combine(directory, $"{App.Edition}.desktop");
        // Written beside its destination and moved onto it, so a write that fails
        // part-way leaves the previous entry whole rather than a truncated one for
        // the desktop database to read.
        var partialPath = entryPath + ".partial";
        try
        {
            Directory.CreateDirectory(directory);
            File.WriteAllText(
                partialPath, DesktopEntryModel.Build(App.Edition, appImagePath), EntryEncoding);
            File.Move(partialPath, entryPath, overwrite: true);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not write the bae desktop entry.", exception);
            DeletePartialEntry(partialPath);
            return;
        }

        RefreshDesktopDatabase(directory);
    }

    // The applications directory under $XDG_DATA_HOME, falling back to the base
    // directory specification's default — both when the variable is unset and when
    // it holds a relative path, which that specification says to treat as invalid.
    private static string ApplicationsDirectory()
    {
        var dataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
        var root = !string.IsNullOrEmpty(dataHome) && Path.IsPathRooted(dataHome)
            ? dataHome
            : Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".local", "share");
        return Path.Combine(root, "applications");
    }

    // Rebuild the desktop MIME database over the directory the entry lives in: the
    // associations are read from that database rather than from the entry, so an
    // entry nothing has indexed is not yet a registration. desktop-file-utils is
    // not guaranteed to be installed — without it the entry is on disk and correct
    // but unassociated until something else rebuilds the database, which the app
    // says rather than passes over.
    private static void RefreshDesktopDatabase(string directory)
    {
        try
        {
            var startInfo = new ProcessStartInfo("update-desktop-database") { UseShellExecute = false };
            startInfo.ArgumentList.Add(directory);
            using var process = Process.Start(startInfo);
            if (process is null)
            {
                return;
            }

            if (!process.WaitForExit(DesktopDatabaseTimeoutMs))
            {
                BaeDiagnostics.Logger.Warning(
                    "update-desktop-database did not finish; the bae desktop entry is not indexed.");
                return;
            }

            if (process.ExitCode != 0)
            {
                BaeDiagnostics.Logger.Warning(
                    $"update-desktop-database exited with {process.ExitCode}; "
                    + "the bae desktop entry is not indexed.");
            }
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning(
                "Could not refresh the desktop database; the bae desktop entry is not indexed.", exception);
        }
    }

    // Clear what a failed write left behind, so nothing partial stays on disk.
    // Best effort: whatever stopped the write can equally stop this.
    private static void DeletePartialEntry(string path)
    {
        try
        {
            File.Delete(path);
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not remove the partial desktop entry.", exception);
        }
    }
}
