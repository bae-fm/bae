using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the album-grid multi-selection semantics: toggle re-anchoring, range
/// extension (including the unresolvable-anchor degrade and unloaded-gap
/// skip), select-all, clear, remove, and the ordered-targets menu/drag rule.
/// Ports the macOS AlbumGridSelectionTests suite over the plain-BCL model.
/// </summary>
public sealed class AlbumGridSelectionModelTests
{
    [Fact]
    public void Toggle_AddsRemovesAndReanchorsOnTheClickedId()
    {
        var selection = new AlbumGridSelectionModel();

        selection.Toggle("a");
        Assert.Equal(new HashSet<string> { "a" }, selection.SelectedIds);
        Assert.Equal("a", selection.AnchorId);

        selection.Toggle("b");
        Assert.Equal(new HashSet<string> { "a", "b" }, selection.SelectedIds);
        Assert.Equal("b", selection.AnchorId);

        selection.Toggle("a");
        Assert.Equal(new HashSet<string> { "b" }, selection.SelectedIds);
        // A ctrl-click re-anchors even when it removes the id.
        Assert.Equal("a", selection.AnchorId);
    }

    [Fact]
    public void ExtendRange_UnionsBothDirectionsFromTheAnchor()
    {
        var ids = new[] { "a", "b", "c", "d", "e" };
        int? Position(string id) => System.Array.IndexOf(ids, id) is var i && i >= 0 ? i : null;
        string? IdAt(int index) => index >= 0 && index < ids.Length ? ids[index] : null;

        var up = new AlbumGridSelectionModel();
        up.Toggle("b");
        up.ExtendRange("d", Position, IdAt);
        Assert.Equal(new HashSet<string> { "b", "c", "d" }, up.SelectedIds);
        // The anchor is unchanged by a range extend.
        Assert.Equal("b", up.AnchorId);

        var down = new AlbumGridSelectionModel();
        down.Toggle("d");
        down.ExtendRange("a", Position, IdAt);
        Assert.Equal(new HashSet<string> { "a", "b", "c", "d" }, down.SelectedIds);
        Assert.Equal("d", down.AnchorId);
    }

    [Fact]
    public void ExtendRange_SkipsIdsInTheSpanThatArentLoaded()
    {
        // Index 2 is within the span but not loaded (idAt returns null).
        var positions = new Dictionary<string, int> { ["a"] = 0, ["b"] = 1, ["d"] = 3 };
        var loaded = new Dictionary<int, string> { [0] = "a", [1] = "b", [3] = "d" };
        var selection = new AlbumGridSelectionModel();

        selection.Toggle("a");
        selection.ExtendRange(
            "d",
            id => positions.TryGetValue(id, out var p) ? p : null,
            index => loaded.TryGetValue(index, out var id) ? id : null);

        Assert.Equal(new HashSet<string> { "a", "b", "d" }, selection.SelectedIds);
    }

    [Fact]
    public void ExtendRange_DegradesToToggleWhenTheAnchorNoLongerResolves()
    {
        var selection = new AlbumGridSelectionModel();
        selection.Toggle("a");
        selection.ExtendRange("c", _ => null, _ => null);

        Assert.Equal(new HashSet<string> { "a", "c" }, selection.SelectedIds);
        Assert.Equal("c", selection.AnchorId);
    }

    [Fact]
    public void SelectAll_SelectsEveryIdAndAnchorsTheLast_ClearEmpties()
    {
        var selection = new AlbumGridSelectionModel();
        selection.SelectAll(new[] { "a", "b", "c" });
        Assert.Equal(new HashSet<string> { "a", "b", "c" }, selection.SelectedIds);
        Assert.Equal("c", selection.AnchorId);

        selection.Clear();
        Assert.True(selection.IsEmpty);
        Assert.Null(selection.AnchorId);
    }

    [Fact]
    public void OrderedTargets_ReturnsVisibleOrderForAMemberOfAMultiSelection()
    {
        var positions = new Dictionary<string, int> { ["a"] = 0, ["b"] = 1, ["c"] = 2 };
        var selection = new AlbumGridSelectionModel();
        selection.Toggle("c");
        selection.Toggle("a");

        Assert.Equal(
            new[] { "a", "c" },
            selection.OrderedTargets("a", id => positions.TryGetValue(id, out var p) ? p : null));
    }

    [Fact]
    public void OrderedTargets_DropsIdsThatDontResolveToAPosition()
    {
        var selection = new AlbumGridSelectionModel();
        selection.Toggle("a");
        selection.Toggle("b");

        Assert.Equal(
            new[] { "a" },
            selection.OrderedTargets("a", id => id == "a" ? 0 : null));
    }

    [Fact]
    public void OrderedTargets_ReturnsJustTheClickedIdForANonMemberClick()
    {
        var positions = new Dictionary<string, int> { ["a"] = 0, ["b"] = 1, ["c"] = 2 };
        var selection = new AlbumGridSelectionModel();
        selection.Toggle("a");
        selection.Toggle("c");

        Assert.Equal(
            new[] { "b" },
            selection.OrderedTargets("b", id => positions.TryGetValue(id, out var p) ? p : null));
    }

    [Fact]
    public void OrderedTargets_ReturnsJustTheClickedIdForASingleSelection()
    {
        var selection = new AlbumGridSelectionModel();
        selection.Toggle("a");

        Assert.Equal(new[] { "a" }, selection.OrderedTargets("a", _ => 0));
    }

    [Fact]
    public void Remove_DropsOnlyTheMissingIdsAndClearsARemovedAnchor()
    {
        var selection = new AlbumGridSelectionModel();
        selection.SelectAll(new[] { "a", "b", "c" });

        selection.Remove(new[] { "b" });
        Assert.Equal(new HashSet<string> { "a", "c" }, selection.SelectedIds);
        // The anchor (c) survives when it isn't among the removed ids.
        Assert.Equal("c", selection.AnchorId);

        selection.Remove(new[] { "c" });
        Assert.Equal(new HashSet<string> { "a" }, selection.SelectedIds);
        Assert.Null(selection.AnchorId);
    }
}
