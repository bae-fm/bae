using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The middle-truncation used on signal-badge values: under-budget passes through,
// an over-budget value keeps both ends around a single ellipsis, and the result
// never exceeds the budget.
public sealed class TextTruncationTests
{
    [Fact]
    public void UnderBudget_PassesThrough()
    {
        Assert.Equal("cat-123", TextTruncation.MiddleTruncate("cat-123", 20));
    }

    [Fact]
    public void AtBudget_PassesThrough()
    {
        var value = new string('x', 12);
        Assert.Equal(value, TextTruncation.MiddleTruncate(value, 12));
    }

    [Fact]
    public void OverBudget_KeepsBothEndsAroundEllipsis()
    {
        // keep = 9, head = 4, tail = 5: first 4 chars, ellipsis, last 5 chars.
        Assert.Equal("ABCD…VWXYZ", TextTruncation.MiddleTruncate("ABCDEFGHIJKLMNOPQRSTUVWXYZ", 10));
    }

    [Fact]
    public void OverBudget_ResultLengthWithinBudget()
    {
        var value = new string('a', 40) + new string('b', 40);
        var result = TextTruncation.MiddleTruncate(value, 21);
        // The ellipsis counts as one character, so the visible length is the budget.
        Assert.Equal(21, result.Length);
        Assert.Contains("…", result);
    }

    [Fact]
    public void OverBudget_TailIsAtLeastHead()
    {
        // keep = maxChars - 1; head = keep / 2; tail = keep - head, so an odd
        // remaining budget gives the tail the extra character.
        var result = TextTruncation.MiddleTruncate("0123456789abcdef", 8);
        // keep = 7, head = 3, tail = 4.
        Assert.Equal("012…cdef", result);
    }
}
