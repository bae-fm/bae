using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// Intersecting several lists keeps the first list's order (and duplicates), drops
// anything absent from any list, and handles the empty and disjoint cases.
public sealed class OrderedIntersectionTests
{
    private static List<T> Intersect<T>(params IReadOnlyList<T>[] lists) =>
        OrderedIntersection.Intersect((IReadOnlyList<IReadOnlyList<T>>)lists);

    [Fact]
    public void OrderFollowsFirstList()
    {
        var result = Intersect(
            new[] { "pin", "unpin", "manage", "unmanage" },
            new[] { "unmanage", "manage", "pin" });
        Assert.Equal(new[] { "pin", "manage", "unmanage" }, result);
    }

    [Fact]
    public void EmptyInput_ReturnsEmpty()
    {
        Assert.Empty(OrderedIntersection.Intersect(new List<IReadOnlyList<string>>()));
    }

    [Fact]
    public void DisjointSets_ReturnEmpty()
    {
        var result = Intersect(new[] { "a", "b" }, new[] { "c", "d" });
        Assert.Empty(result);
    }

    [Fact]
    public void SingleList_ReturnsItself()
    {
        var result = Intersect(new[] { "a", "b", "c" });
        Assert.Equal(new[] { "a", "b", "c" }, result);
    }

    [Fact]
    public void Duplicates_InFirstListArePreserved()
    {
        var result = Intersect(new[] { "a", "a", "b" }, new[] { "a", "b" });
        Assert.Equal(new[] { "a", "a", "b" }, result);
    }
}
