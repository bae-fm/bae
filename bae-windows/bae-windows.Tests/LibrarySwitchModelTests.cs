using System.Collections.Generic;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the digit → library-id mapping behind the Ctrl+1..9 library-switch
/// accelerators: positional addressing, the active-target and out-of-range
/// no-ops, and the empty-list case.
/// </summary>
public sealed class LibrarySwitchModelTests
{
    private static IReadOnlyList<(string Id, bool IsActive)> Libraries(int activeIndex, params string[] ids)
    {
        var list = new List<(string Id, bool IsActive)>();
        for (var i = 0; i < ids.Length; i++)
        {
            list.Add((ids[i], i == activeIndex));
        }
        return list;
    }

    [Fact]
    public void Digit1_ReturnsFirstLibrary()
    {
        var libraries = Libraries(activeIndex: 1, "lib-a", "lib-b", "lib-c");
        Assert.Equal("lib-a", LibrarySwitchModel.TargetLibraryId(libraries, 1));
    }

    [Fact]
    public void DigitOnActiveLibrary_ReturnsNull()
    {
        var libraries = Libraries(activeIndex: 1, "lib-a", "lib-b", "lib-c");
        Assert.Null(LibrarySwitchModel.TargetLibraryId(libraries, 2));
    }

    [Fact]
    public void DigitBeyondList_ReturnsNull()
    {
        var libraries = Libraries(activeIndex: 0, "lib-a", "lib-b");
        Assert.Null(LibrarySwitchModel.TargetLibraryId(libraries, 3));
    }

    [Theory]
    [InlineData(0)]
    [InlineData(10)]
    public void DigitOutsideDomain_ReturnsNull(int digit)
    {
        var libraries = Libraries(activeIndex: 0, "lib-a", "lib-b", "lib-c");
        Assert.Null(LibrarySwitchModel.TargetLibraryId(libraries, digit));
    }

    [Theory]
    [InlineData(1)]
    [InlineData(5)]
    [InlineData(9)]
    public void EmptyList_ReturnsNull(int digit)
    {
        var libraries = new List<(string Id, bool IsActive)>();
        Assert.Null(LibrarySwitchModel.TargetLibraryId(libraries, digit));
    }

    [Fact]
    public void AddressingIsPositional_IndependentOfActiveEntry()
    {
        var libraries = Libraries(activeIndex: 2, "lib-a", "lib-b", "lib-c", "lib-d");
        Assert.Equal("lib-a", LibrarySwitchModel.TargetLibraryId(libraries, 1));
        Assert.Equal("lib-b", LibrarySwitchModel.TargetLibraryId(libraries, 2));
        Assert.Equal("lib-d", LibrarySwitchModel.TargetLibraryId(libraries, 4));
    }
}
