using System.Collections.Generic;
using System.Linq;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

public sealed class ReleaseQueueInteractionModelTests
{
    [Fact]
    public void NewDisclosureStartsExpandedAndSetIsAbsolute()
    {
        var model = new ReleaseQueueInteractionModel();
        var key = new ReleaseGroupDisclosureKey(
            "/music",
            "first");

        Assert.True(model.IsGroupExpanded(key));
        model.SetGroupExpanded(key, false);
        model.SetGroupExpanded(key, false);
        Assert.False(model.IsGroupExpanded(key));
        model.SetGroupExpanded(key, true);
        Assert.True(model.IsGroupExpanded(key));
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

        Assert.True(model.IsGroupExpanded(stale));
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
    public void EveryGroupedSectionSortsByRenderedTitleInBothDirections()
    {
        var groups = new[]
        {
            new[] { "Zulu", "Alpha", "Middle" },
            new[] { "Charlie", "Bravo" },
        };

        var ascending = groups
            .Select(group => ReleaseQueueSortModel.Sort(group, title => title, false))
            .ToList();
        var descending = groups
            .Select(group => ReleaseQueueSortModel.Sort(group, title => title, true))
            .ToList();

        Assert.Equal(new[] { "Alpha", "Middle", "Zulu" }, ascending[0]);
        Assert.Equal(new[] { "Bravo", "Charlie" }, ascending[1]);
        Assert.Equal(new[] { "Zulu", "Middle", "Alpha" }, descending[0]);
        Assert.Equal(new[] { "Charlie", "Bravo" }, descending[1]);
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

    [Fact]
    public void CandidateSelectionSurvivesWhenDecisionKeepsItsRow()
    {
        var retained = CandidateSelectionModel.Retain(
            "/music/group/release",
            new HashSet<string> { "/music/group/release", "/music/other" });

        Assert.Equal("/music/group/release", retained);
    }

    [Fact]
    public void CandidateSelectionClearsWhenDecisionReplacesItsRow()
    {
        var retained = CandidateSelectionModel.Retain(
            "/music/group/disc-1",
            new HashSet<string> { "/music/group" });

        Assert.Null(retained);
    }

}
