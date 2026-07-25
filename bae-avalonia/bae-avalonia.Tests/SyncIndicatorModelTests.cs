using System;
using System.Globalization;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the last-sync timestamp format for the toolbar's Synced badge. The
/// badge's precedence is decided in bae-core (bae-core's `sync_indicator_tests`)
/// and reached through BridgeSyncIndicator; only the time rendering is the UI's.
/// Timestamp cases pin an explicit culture, as the Loc formatter tests do.
/// </summary>
public sealed class SyncIndicatorModelTests
{
    private static T WithCulture<T>(string name, Func<T> f)
    {
        var culture = CultureInfo.GetCultureInfo(name);
        var prevCulture = CultureInfo.CurrentCulture;
        CultureInfo.CurrentCulture = culture;
        try
        {
            return f();
        }
        finally
        {
            CultureInfo.CurrentCulture = prevCulture;
        }
    }

    // ── FormatSyncTime ──

    [Fact]
    public void FormatSyncTime_NullIsNull() =>
        Assert.Null(SyncIndicatorModel.FormatSyncTime(null));

    [Theory]
    [InlineData("en-US")]
    [InlineData("de-DE")]
    public void FormatSyncTime_IsLocalShortTimeUnderCulture(string culture)
    {
        // A fixed instant renders through the short-time ("t") pattern in the
        // current culture; recomputed under the same culture so the assertion is
        // timezone-neutral while still proving the culture is applied.
        const long ms = 1_700_000_000_000;
        var result = WithCulture(culture, () => SyncIndicatorModel.FormatSyncTime(ms));
        var expected = WithCulture(culture, () =>
            DateTimeOffset.FromUnixTimeMilliseconds(ms).ToLocalTime().ToString("t"));
        Assert.NotNull(result);
        Assert.Equal(expected, result);
    }
}
