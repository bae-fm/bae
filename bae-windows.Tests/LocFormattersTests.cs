using System;
using System.Globalization;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the behavior of the locale-aware value formatters in <see cref="Loc"/>
/// — the reimplementation of macOS's ByteCountFormatter (byte counts) and
/// DateComponentsFormatter (durations), plus locale-grouped whole numbers.
/// Every case pins an explicit culture so the assertions don't depend on the
/// host's default locale.
/// </summary>
public sealed class LocFormattersTests
{
    /// <summary>Run <paramref name="f"/> with the current thread's culture set
    /// to <paramref name="name"/>, restoring it afterward. CurrentCulture is
    /// per-thread, so this stays isolated across parallel test classes.</summary>
    private static string WithCulture(string name, Func<string> f)
    {
        var culture = CultureInfo.GetCultureInfo(name);
        var prevCulture = CultureInfo.CurrentCulture;
        var prevUiCulture = CultureInfo.CurrentUICulture;
        CultureInfo.CurrentCulture = culture;
        CultureInfo.CurrentUICulture = culture;
        try
        {
            return f();
        }
        finally
        {
            CultureInfo.CurrentCulture = prevCulture;
            CultureInfo.CurrentUICulture = prevUiCulture;
        }
    }

    // ── Bytes: 1000-based (SI) ladder, whole bytes with no decimal, larger
    //    units with one decimal, negatives clamped to zero. ────────────────────

    [Theory]
    [InlineData(0, "0 B")]
    [InlineData(1, "1 B")]
    [InlineData(512, "512 B")]
    [InlineData(999, "999 B")] // stays in bytes right up to the 1000 threshold
    [InlineData(1000, "1.0 KB")] // decimal ladder: 1000, not 1024, steps the unit
    [InlineData(1500, "1.5 KB")]
    [InlineData(1_000_000, "1.0 MB")]
    [InlineData(412_000_000, "412.0 MB")]
    [InlineData(1_000_000_000, "1.0 GB")]
    [InlineData(1_000_000_000_000, "1.0 TB")]
    public void Bytes_EnUs(long bytes, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Bytes(bytes)));

    [Fact]
    public void Bytes_NegativeClampsToZero() =>
        Assert.Equal("0 B", WithCulture("en-US", () => Loc.Bytes(-1)));

    [Fact]
    public void Bytes_CapsAtTerabytes() =>
        // 10^15 is a petabyte; the ladder stops at TB, so it reads as 1000 TB.
        Assert.Equal("1000.0 TB", WithCulture("en-US", () => Loc.Bytes(1_000_000_000_000_000)));

    [Theory]
    [InlineData(1440, "1.4 KB")] // 1.44 rounds down
    [InlineData(1460, "1.5 KB")] // 1.46 rounds up
    public void Bytes_RoundsToOneDecimal(long bytes, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Bytes(bytes)));

    [Fact]
    public void Bytes_UsesCultureDecimalSeparator() =>
        // German uses a comma as the decimal separator.
        Assert.Equal("1,5 KB", WithCulture("de-DE", () => Loc.Bytes(1500)));

    [Fact]
    public void Bytes_WholeBytesHaveNoDecimalRegardlessOfCulture() =>
        Assert.Equal("999 B", WithCulture("de-DE", () => Loc.Bytes(999)));

    // ── Duration: minutes:seconds, rolling into hours; null → empty; negatives
    //    clamped to zero. ──────────────────────────────────────────────────────

    [Fact]
    public void Duration_NullIsEmpty() =>
        Assert.Equal(string.Empty, WithCulture("en-US", () => Loc.Duration(null)));

    [Theory]
    [InlineData(0, "0:00")]
    [InlineData(7_000, "0:07")] // seconds zero-padded to two digits
    [InlineData(67_000, "1:07")]
    [InlineData(187_000, "3:07")]
    [InlineData(599_000, "9:59")]
    [InlineData(3_599_000, "59:59")] // last second before an hour accrues
    public void Duration_MinutesSeconds(long ms, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Duration(ms)));

    [Theory]
    [InlineData(3_600_000, "1:00:00")] // exactly one hour switches to h:mm:ss
    [InlineData(3_661_000, "1:01:01")]
    [InlineData(3_723_000, "1:02:03")]
    [InlineData(5_999_000, "1:39:59")]
    [InlineData(36_000_000, "10:00:00")]
    public void Duration_HoursMinutesSeconds(long ms, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Duration(ms)));

    [Fact]
    public void Duration_NegativeClampsToZero() =>
        Assert.Equal("0:00", WithCulture("en-US", () => Loc.Duration(-5_000)));

    [Fact]
    public void Duration_SubSecondFloorsToZeroSeconds() =>
        Assert.Equal("0:00", WithCulture("en-US", () => Loc.Duration(400)));

    [Fact]
    public void Duration_FieldSeparatorsAreCultureStable() =>
        // No decimal/grouping is involved, so a comma-decimal culture renders
        // the same m:ss form.
        Assert.Equal("3:07", WithCulture("de-DE", () => Loc.Duration(187_000)));

    // ── Number: locale-grouped whole numbers (N0). ─────────────────────────────

    [Theory]
    [InlineData(0, "0")]
    [InlineData(999, "999")]
    [InlineData(1_000, "1,000")]
    [InlineData(1_234_567, "1,234,567")]
    [InlineData(-1_234, "-1,234")]
    public void Number_EnUs(long value, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Number(value)));

    [Fact]
    public void Number_UsesCultureGroupingSeparator() =>
        // German groups with a period.
        Assert.Equal("1.234.567", WithCulture("de-DE", () => Loc.Number(1_234_567)));
}
