using System;

namespace Bae.Windows;

// The last-sync timestamp format for the toolbar's Synced badge. The badge's
// precedence (error > syncing > synced > idle) is decided in bae-core and
// reached through BridgeSyncIndicator; only the rendering of the time is the
// UI's, which is all that remains here.
public static class SyncIndicatorModel
{
    // Format a Unix epoch-millis sync timestamp as a local short time ("2:32 PM"),
    // or null when there's been no sync.
    public static string? FormatSyncTime(long? epochMillis)
    {
        if (epochMillis is not long ms)
        {
            return null;
        }

        return DateTimeOffset.FromUnixTimeMilliseconds(ms).ToLocalTime().ToString("t");
    }
}
