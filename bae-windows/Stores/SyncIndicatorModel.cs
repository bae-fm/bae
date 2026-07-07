using System;

namespace Bae.Windows;

// What the toolbar sync indicator shows, chosen by precedence.
public enum SyncIndicatorKind
{
    Error,
    Syncing,
    Synced,
    Blank,
}

public sealed record SyncIndicatorText(SyncIndicatorKind Kind, string? LastSyncTime);

// Pure logic behind the toolbar sync indicator: which state wins, and the
// last-sync timestamp format. No WinUI — the caller maps the kind to a label
// and colour.
public static class SyncIndicatorModel
{
    // An active error wins, then an in-progress pass, then the last successful
    // sync time; blank when there's nothing to report.
    public static SyncIndicatorText Resolve(bool hasError, bool syncing, string? lastSyncTime)
    {
        if (hasError)
        {
            return new SyncIndicatorText(SyncIndicatorKind.Error, null);
        }
        if (syncing)
        {
            return new SyncIndicatorText(SyncIndicatorKind.Syncing, null);
        }
        if (lastSyncTime is not null)
        {
            return new SyncIndicatorText(SyncIndicatorKind.Synced, lastSyncTime);
        }
        return new SyncIndicatorText(SyncIndicatorKind.Blank, null);
    }

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
