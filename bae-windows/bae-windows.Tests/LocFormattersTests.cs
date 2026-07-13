using System;
using System.Globalization;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the behavior of the locale-aware value formatters in <see cref="Loc"/>
/// — the reimplementation of macOS's ByteCountFormatter (byte counts), the clock
/// renderer over the fields core hands across the bridge, and locale-grouped
/// whole numbers. Every case pins an explicit culture so the assertions don't
/// depend on the host's default locale.
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

    // ── Clock: the digits of the fields core hands over. Which fields exist —
    //    whether there is an hours field, whether there is a clock at all — is
    //    core's decision and is pinned by bae-core's `util::duration` tests; what
    //    is pinned here is the rendering: separators, padding, sign, culture. ───

    [Theory]
    [InlineData(0u, 0u, "0:00")]
    [InlineData(0u, 7u, "0:07")] // seconds zero-padded to two digits
    [InlineData(3u, 7u, "3:07")] // the leading field is not padded
    [InlineData(59u, 59u, "59:59")]
    public void Clock_MinutesSeconds(uint minutes, uint seconds, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Clock(false, null, minutes, seconds)));

    [Theory]
    [InlineData(1ul, 0u, 0u, "1:00:00")]
    [InlineData(1ul, 2u, 3u, "1:02:03")] // minutes pad once they follow an hours field
    [InlineData(10ul, 0u, 0u, "10:00:00")]
    public void Clock_HoursMinutesSeconds(ulong hours, uint minutes, uint seconds, string expected) =>
        Assert.Equal(expected, WithCulture("en-US", () => Loc.Clock(false, hours, minutes, seconds)));

    [Fact]
    public void Clock_NegativeCountsDownWithAMinusPrefix() =>
        Assert.Equal("-2:30", WithCulture("en-US", () => Loc.Clock(true, null, 2, 30)));

    [Fact]
    public void Clock_NegativeZeroKeepsTheMinus() =>
        Assert.Equal("-0:00", WithCulture("en-US", () => Loc.Clock(true, null, 0, 0)));

    [Fact]
    public void Clock_FieldSeparatorsAreCultureStable() =>
        // No decimal/grouping is involved, so a comma-decimal culture renders
        // the same m:ss form.
        Assert.Equal("3:07", WithCulture("de-DE", () => Loc.Clock(false, null, 3, 7)));

    [Fact]
    public void Clock_LongHoursFieldIsNotGrouped() =>
        // The leading field is a count of hours, not a quantity: "100:00:00",
        // never "1,000:00:00" at four digits.
        Assert.Equal("1000:00:00", WithCulture("en-US", () => Loc.Clock(false, 1000, 0, 0)));

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
