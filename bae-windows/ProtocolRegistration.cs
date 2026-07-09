using Microsoft.Win32;

namespace Bae.Windows;

/// <summary>
/// Per-user (<c>HKCU\Software\Classes</c>) registration of the <c>bae://</c>
/// protocol and the folder "open with bae" verb — the unpackaged-app
/// equivalent of macOS's <c>CFBundleURLTypes</c> / <c>CFBundleDocumentTypes</c>
/// declarations, written by hand because an unpackaged, unsigned app has no
/// package identity to declare them through. No elevation and no signing:
/// every key lives under the current user's hive.
/// </summary>
internal static class ProtocolRegistration
{
    private const string ProtocolKeyPath = @"Software\Classes\bae";
    private const string FolderVerbKeyPath = @"Software\Classes\Directory\shell\bae";

    /// <summary>
    /// Write both key trees, overwriting whatever is already there. Called on
    /// every launch (gated by the caller on this being a Velopack install) so
    /// the command always points at the current <c>current\</c> exe path and
    /// the verb label re-resolves after a locale change. A write failure (the
    /// hive is unreadable, some policy blocks it) is logged and leaves
    /// whatever registration already existed in place.
    /// </summary>
    internal static void Register()
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
        var editionName = NativeBae.SupportsOAuthProviders() ? "bae" : "baeium";

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
            folderVerbKey.SetValue(null, Loc.Chrome("shell.open_with_folder", "name", editionName));
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

    /// <summary>Remove both key trees. Run from the Velopack pre-uninstall
    /// callback, before any UI/resource initialization — needs no localized
    /// strings, so it is safe there.</summary>
    internal static void Unregister()
    {
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
}
