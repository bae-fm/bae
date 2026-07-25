using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

// The re-identify results list arbitration: the pipeline owns the list until a
// manual search takes it, and a pipeline render is skipped when its match set is
// unchanged so the user's in-list selection survives a badge-only refresh.
public sealed class ReidentifyResultsModelTests
{
    private static List<string> Keys(params string[] keys) => new(keys);

    [Fact]
    public void EmptyToEmptyRefresh_DoesNotRender()
    {
        var model = new ReidentifyResultsModel();
        Assert.False(model.ApplyPipelineMatches(Keys()));
    }

    [Fact]
    public void FirstNonEmptySet_Renders()
    {
        var model = new ReidentifyResultsModel();
        Assert.True(model.ApplyPipelineMatches(Keys("rel-1", "rel-2")));
    }

    [Fact]
    public void IdenticalReplay_DoesNotRender()
    {
        var model = new ReidentifyResultsModel();
        model.ApplyPipelineMatches(Keys("rel-1", "rel-2"));
        Assert.False(model.ApplyPipelineMatches(Keys("rel-1", "rel-2")));
    }

    [Fact]
    public void ChangedSet_Renders()
    {
        var model = new ReidentifyResultsModel();
        model.ApplyPipelineMatches(Keys("rel-1", "rel-2"));
        Assert.True(model.ApplyPipelineMatches(Keys("rel-1", "rel-3")));
    }

    [Fact]
    public void ManualResults_FreezePipelineRenders()
    {
        var model = new ReidentifyResultsModel();
        model.ApplyManualResults();
        Assert.True(model.ManualResultsShown);
        // Even a brand-new match set is held back while manual results own the list.
        Assert.False(model.ApplyPipelineMatches(Keys("rel-1")));
        Assert.False(model.ApplyPipelineMatches(Keys("rel-2", "rel-3")));
    }

    [Fact]
    public void ResumePipeline_ClearsManualFlagAndChangeCache()
    {
        var model = new ReidentifyResultsModel();
        model.ApplyPipelineMatches(Keys("rel-1", "rel-2"));
        model.ApplyManualResults();

        model.ResumePipeline();
        Assert.False(model.ManualResultsShown);
        // The change cache is cleared too: a refresh whose set equals the last
        // pipeline render still re-renders after a resume.
        Assert.True(model.ApplyPipelineMatches(Keys("rel-1", "rel-2")));
    }
}
