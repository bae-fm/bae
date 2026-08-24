using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Which watched-folder scan failures have already been put in front of the
/// user, so one broken folder raises one dialog.
///
/// The failures arrive on the import list's summary — the scan writes each to
/// <c>folder_scan_roots</c> and the live query reads it back — rather than as a
/// transient event, so a scan that failed while the app was still starting up
/// is in the first delivery instead of having been published to nobody. That
/// also means the same failure arrives over and over: the summary is
/// re-delivered on every verdict the sweep commits, and the timer re-reads
/// every root every quarter hour, failing the same way each time on a folder
/// that cannot be read. This is what tells a repeat from news.
/// </summary>
internal sealed class ScanFailureAlerts
{
    private Dictionary<string, string> _reported = new();

    /// <summary>
    /// The roots in <paramref name="statuses"/> whose failure has not been
    /// shown yet — each with the untranslated fault. Rebuilds what stands from
    /// the delivery, so a root that reads cleanly again, or stops being
    /// watched, leaves and its next break counts as news.
    /// </summary>
    public IReadOnlyList<(string Path, string Detail)> NewFailures(
        IEnumerable<BridgeWatchedFolderScanStatus> statuses)
    {
        var current = new Dictionary<string, string>();
        var raised = new List<(string, string)>();
        foreach (var status in statuses)
        {
            if (status.Status is not BridgeFolderScanStatus.Failed failed)
            {
                continue;
            }
            current[status.WatchedFolderPath] = failed.Error;
            if (!_reported.TryGetValue(status.WatchedFolderPath, out var shown)
                || shown != failed.Error)
            {
                raised.Add((status.WatchedFolderPath, failed.Error));
            }
        }
        _reported = current;
        return raised;
    }

    /// <summary>Forget everything shown, for a store being handed a different
    /// library. The next library's broken folder is news of its own.</summary>
    public void Clear() => _reported = new Dictionary<string, string>();
}
