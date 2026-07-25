using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The queue-reorder math: an entry moved within a lane lands before whatever now
// follows it, or at the lane's end (null) when it is now last.
public sealed class QueueReorderModelTests
{
    [Fact]
    public void MoveToMiddle_LandsBeforeNextEntry()
    {
        var lane = new List<string> { "a", "b", "c", "d" };
        var move = QueueReorderModel.ResolveMove(lane, 1);
        Assert.Equal("b", move.MovedEntryId);
        Assert.Equal("c", move.BeforeEntryId);
    }

    [Fact]
    public void MoveToEnd_HasNoFollowingEntry()
    {
        var lane = new List<string> { "a", "b", "c" };
        var move = QueueReorderModel.ResolveMove(lane, 2);
        Assert.Equal("c", move.MovedEntryId);
        Assert.Null(move.BeforeEntryId);
    }

    [Fact]
    public void SingleItemLane_MovesToEnd()
    {
        var lane = new List<string> { "only" };
        var move = QueueReorderModel.ResolveMove(lane, 0);
        Assert.Equal("only", move.MovedEntryId);
        Assert.Null(move.BeforeEntryId);
    }

    [Fact]
    public void MoveToFront_LandsBeforeFormerFirst()
    {
        var lane = new List<string> { "moved", "a", "b" };
        var move = QueueReorderModel.ResolveMove(lane, 0);
        Assert.Equal("moved", move.MovedEntryId);
        Assert.Equal("a", move.BeforeEntryId);
    }
}
