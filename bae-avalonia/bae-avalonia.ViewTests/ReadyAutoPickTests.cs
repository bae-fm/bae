using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>ReadyAutoPick decides when a selected row opens the mapping pane on
/// its settled match without a click. Its guards are what keep the seed from
/// firing where a pick would be wrong: rows outside Ready, panes that already
/// hold an answer, and prefetches that failed once.</summary>
public sealed class ReadyAutoPickTests
{
    private const string CandidateKey = "/Music/Incoming/Collection/Release 01";

    private static readonly MappingPaneSeedState Untouched =
        new(Seeded: false, Prefetching: false, PrefetchFailed: false);

    [Fact]
    public void SeedsFromAReadyRow()
    {
        var pick = ReadyAutoPick.From(
            Row(new BridgeTriagePlacement.Ready(), Match("rel-ready")),
            Untouched);

        Assert.Equal("rel-ready", pick?.ReleaseId);
    }

    [Fact]
    public void IgnoresRowsOutsideReady()
    {
        // Done and Skipped rows carry a match too, and both rebuild into a pane
        // that has picked nothing — placement is the only thing standing between
        // an imported folder and a re-opened commit bar.
        foreach (var placement in new BridgeTriagePlacement[]
        {
            new BridgeTriagePlacement.Done(),
            new BridgeTriagePlacement.Skipped(),
        })
        {
            Assert.Null(ReadyAutoPick.From(Row(placement, Match("rel-done")), Untouched));
        }
    }

    [Fact]
    public void IgnoresAnUnsettledLead()
    {
        // Several matches: the row leads with one of them, but which pressing is
        // exactly the open question — core withholds the pressing, so there is
        // nothing settled to seed.
        var placement = new BridgeTriagePlacement.NeedsYou(
            BridgeNeedsYouGroup.PickAPressing,
            new BridgeNeedsYouReason.Disagreement(new BridgeNeedsYou.SeveralMatches(4)));

        Assert.Null(ReadyAutoPick.From(Row(placement, Match("rel-lead")), Untouched));
    }

    [Fact]
    public void YieldsToAPickAlreadyIn()
    {
        var seeded = Untouched with { Seeded = true };

        Assert.Null(ReadyAutoPick.From(
            Row(new BridgeTriagePlacement.Ready(), Match("rel-ready")),
            seeded));
    }

    [Fact]
    public void YieldsToAPrefetchInFlight()
    {
        var running = Untouched with { Prefetching = true };

        Assert.Null(ReadyAutoPick.From(
            Row(new BridgeTriagePlacement.Ready(), Match("rel-ready")),
            running));
    }

    [Fact]
    public void StaysDownAfterAFailedPrefetch()
    {
        // A failed prefetch leaves the pane holding nothing, so without this
        // guard the next queue tick would seed the same read again, and on a
        // persistent failure, forever.
        var failed = Untouched with { PrefetchFailed = true };

        Assert.Null(ReadyAutoPick.From(
            Row(new BridgeTriagePlacement.Ready(), Match("rel-ready")),
            failed));
    }

    [Fact]
    public void ReadyWithoutAMatchSeedsNothing()
    {
        Assert.Null(ReadyAutoPick.From(
            Row(new BridgeTriagePlacement.Ready(), matched: null),
            Untouched));
    }

    [Fact]
    public void AbsentRowSeedsNothing()
    {
        Assert.Null(ReadyAutoPick.From(row: null, Untouched));
    }

    private static BridgeMatchedRelease Match(string releaseId) => new(
        ReleaseId: releaseId,
        Title: "Album Title",
        Artist: "Artist Name",
        Pressing: null,
        CoverThumbnailUrl: null,
        Evidence: new BridgeMatchEvidence(
            BridgeMetadataSource.MusicBrainz,
            BridgeMatchedSignal.DiscId),
        Claim: new BridgeIdentityChoice.Exact(releaseId, BridgeMetadataSource.MusicBrainz));

    private static BridgeTriageRow Row(
        BridgeTriagePlacement placement,
        BridgeMatchedRelease? matched) => new(
        CandidateKey: CandidateKey,
        FolderName: "Release 01",
        WatchedFolderPath: "/Music/Incoming",
        DisplayPath: "Collection/Release 01",
        ResolvedBoundaries: System.Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
        CombineAncestorKey: null,
        Actionable: true,
        Placement: placement,
        Matched: matched,
        Selectable: placement is BridgeTriagePlacement.Ready,
        ImportStatus: null);
}
