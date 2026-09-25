using Velopack;
using Velopack.Sources;

namespace Bae.Desktop;

/// <summary>
/// The in-app updater over the Velopack release feed: at launch it checks the
/// GitHub releases, downloads a newer package, and stages it to apply when the
/// app exits.
/// </summary>
internal static class UpdateService
{
    // The public repository's GitHub releases are the feed. Anonymous access is
    // one API call per check; the installed package's channel selects its own
    // releases.<channel>.json, so no edition branching is needed here.
    private static readonly UpdateManager Manager = new(
        new GithubSource("https://github.com/bae-fm/bae", null, false));

    /// <summary>Download and stage an available update to apply on the next
    /// exit. Any failure (offline, unreachable feed, checksum mismatch) is
    /// logged. No-op off a Velopack install — a dev run or a loose copy, where
    /// the Velopack calls are invalid.</summary>
    internal static async Task StageInBackgroundAsync()
    {
        if (!Manager.IsInstalled)
        {
            return;
        }

        try
        {
            var info = await Manager.CheckForUpdatesAsync();
            if (info is null)
            {
                return;
            }

            await Manager.DownloadUpdatesAsync(info);
            Manager.WaitExitThenApplyUpdates(info.TargetFullRelease, silent: true, restart: false);
            BaeDiagnostics.Logger.Info(
                $"Update {info.TargetFullRelease.Version} staged; it applies when the app exits.");
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Background update check failed.", exception);
        }
    }
}
