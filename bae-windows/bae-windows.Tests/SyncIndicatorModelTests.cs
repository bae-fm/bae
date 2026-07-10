using System;
using System.Globalization;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the toolbar sync indicator's precedence (error &gt; syncing &gt;
/// last-sync &gt; blank) and the last-sync timestamp format. Timestamp cases pin
/// an explicit culture, as the Loc formatter tests do.
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

    // ── Precedence: error > syncing > last-sync > blank ──

    [Fact]
    public void Resolve_ErrorWinsOverEverything()
    {
        var text = SyncIndicatorModel.Resolve(hasError: true, syncing: true, lastSyncTime: "2:32 PM");
        Assert.Equal(SyncIndicatorKind.Error, text.Kind);
        Assert.Null(text.LastSyncTime);
    }

    [Fact]
    public void Resolve_SyncingWinsOverLastSync()
    {
        var text = SyncIndicatorModel.Resolve(hasError: false, syncing: true, lastSyncTime: "2:32 PM");
        Assert.Equal(SyncIndicatorKind.Syncing, text.Kind);
        Assert.Null(text.LastSyncTime);
    }

    [Fact]
    public void Resolve_SyncedCarriesTime()
    {
        var text = SyncIndicatorModel.Resolve(hasError: false, syncing: false, lastSyncTime: "2:32 PM");
        Assert.Equal(SyncIndicatorKind.Synced, text.Kind);
        Assert.Equal("2:32 PM", text.LastSyncTime);
    }

    [Fact]
    public void Resolve_BlankWhenNothing()
    {
        var text = SyncIndicatorModel.Resolve(hasError: false, syncing: false, lastSyncTime: null);
        Assert.Equal(SyncIndicatorKind.Blank, text.Kind);
        Assert.Null(text.LastSyncTime);
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
