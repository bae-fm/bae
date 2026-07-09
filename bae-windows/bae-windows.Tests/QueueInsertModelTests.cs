using System.Collections.Generic;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

// The queue drag payload encodes ids newline-joined and decodes back to the same
// list, dropping empty segments so a stray separator never yields a blank id.
public sealed class QueueDragPayloadTests
{
    [Fact]
    public void RoundTrips_MultipleIds()
    {
        var ids = new List<string> { "id-one", "id-two", "id-three" };
        var decoded = QueueDragPayload.Decode(QueueDragPayload.Encode(ids));
        Assert.Equal(ids, decoded);
    }

    [Fact]
    public void SingleId_EncodesToBareId()
    {
        Assert.Equal("only-id", QueueDragPayload.Encode(new List<string> { "only-id" }));
        Assert.Equal(new List<string> { "only-id" }, QueueDragPayload.Decode("only-id"));
    }

    [Fact]
    public void Decode_OmitsEmptySegments()
    {
        Assert.Equal(
            new List<string> { "a", "b" },
            QueueDragPayload.Decode("a\n\nb\n"));
    }

    [Fact]
    public void EmptyList_EncodesAndDecodesEmpty()
    {
        Assert.Equal(string.Empty, QueueDragPayload.Encode(new List<string>()));
        Assert.Empty(QueueDragPayload.Decode(string.Empty));
    }
}

// The insertion index for a drop over the manual lane: before the first row whose
// midpoint sits below the pointer, else appended at the lane's end.
public sealed class QueueDropIndexTests
{
    // Three rows, 40px tall, top-aligned: midpoints at 20, 60, 100.
    private static readonly List<RealizedRow> ThreeRows = new()
    {
        new RealizedRow(0, 20),
        new RealizedRow(1, 60),
        new RealizedRow(2, 100),
    };

    [Fact]
    public void EmptyLane_InsertsAtZero()
    {
        Assert.Equal(0, QueueDropIndex.Insert(new List<RealizedRow>(), 50, 0));
    }

    [Fact]
    public void AboveFirstMidpoint_InsertsBeforeFirst()
    {
        Assert.Equal(0, QueueDropIndex.Insert(ThreeRows, 5, 3));
    }

    [Fact]
    public void BetweenRows_InsertsBeforeFollowingRow()
    {
        // Below row 0's midpoint (20) but above row 1's (60): lands before row 1.
        Assert.Equal(1, QueueDropIndex.Insert(ThreeRows, 40, 3));
    }

    [Fact]
    public void BelowAllMidpoints_Appends()
    {
        Assert.Equal(3, QueueDropIndex.Insert(ThreeRows, 200, 3));
    }

    [Fact]
    public void SparseRealizedRows_UsesRealizedIndices()
    {
        // Only rows 2 and 5 are realized (the rest virtualized out); a pointer
        // above row 5's midpoint but below row 2's lands before row 5.
        var sparse = new List<RealizedRow>
        {
            new RealizedRow(2, 30),
            new RealizedRow(5, 90),
        };
        Assert.Equal(5, QueueDropIndex.Insert(sparse, 60, 8));
    }
}
