using System.Collections.Generic;
using System.Linq;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

public sealed class ReleaseQueueInteractionModelTests
{
    // A group nobody has folded is not in the collapsed set core is given, and
    // setting the state is absolute: the caller passes what the control
    // represents, not a toggle.
    [Fact]
    public void NewDisclosureStartsExpandedAndSetIsAbsolute()
    {
        var model = new ReleaseQueueInteractionModel();
        var key = new ReleaseGroupDisclosureKey(
            "/music",
            "first");

        Assert.Empty(model.CollapsedKeys());
        model.SetGroupExpanded(key, false);
        model.SetGroupExpanded(key, false);
        Assert.Equal(new[] { key }, model.CollapsedKeys());
        model.SetGroupExpanded(key, true);
        Assert.Empty(model.CollapsedKeys());
    }

    [Fact]
    public void DisclosureStateIsNamedAndTypedForReleaseGroups()
    {
        var key = new ReleaseGroupDisclosureKey("/music", "collection");

        Assert.Equal("/music", key.WatchedRoot);
        Assert.Equal("collection", key.RelativePath);
    }

    [Fact]
    public void ProjectionRefreshDropsAbsentDisclosureState()
    {
        var model = new ReleaseQueueInteractionModel();
        var stale = new ReleaseGroupDisclosureKey(
            "/music",
            "stale");
        model.SetGroupExpanded(stale, false);

        model.RetainGroupDisclosureKeys(new[]
        {
            new ReleaseGroupDisclosureKey(
                "/music",
                "current"),
        });

        Assert.Empty(model.CollapsedKeys());
    }

    [Fact]
    public void DisclosureKeysDoNotCollideAcrossPathComponentBoundaries()
    {
        var first = new ReleaseGroupDisclosureKey(
            "/music\nnested",
            "release");
        var second = new ReleaseGroupDisclosureKey(
            "/music",
            "nested\nrelease");

        Assert.NotEqual(first, second);
    }

    [Fact]
    public void RefreshStateIsIndependentPerRoot()
    {
        var model = new ReleaseQueueInteractionModel();
        model.SetRefreshing("/music/first", true);
        model.SetRefreshing("/music/second", true);
        model.SetRefreshing("/music/first", false);

        Assert.False(model.IsRefreshing("/music/first"));
        Assert.True(model.IsRefreshing("/music/second"));
    }

    [Fact]
    public void ReadRequestsCoalesceToOneDirtyRerun()
    {
        var model = new CoalescedReadModel();

        Assert.True(model.Request());
        Assert.False(model.Request());
        Assert.False(model.Request());
        Assert.True(model.Complete());
        Assert.False(model.Complete());
        Assert.True(model.Request());
    }

    [Fact]
    public void SelectAllReplacesThePreviousSelection()
    {
        var selection = new HashSet<string> { "stale", "visible-one" };

        ReadySelectionModel.Replace(
            selection,
            new[] { "visible-one", "visible-two" });

        Assert.True(selection.SetEquals(new[] { "visible-one", "visible-two" }));
    }
}
