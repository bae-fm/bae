using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>PickedResume decides when a selected row opens the mapping pane on
/// the identity it already carries — the settled single match, or the choice
/// made before a restart — without a click. Its guards are what keep the
/// resume from firing where it would be wrong: rows past deciding, panes that
/// already hold an answer, and prefetches that failed once.</summary>
public sealed class PickedResumeTests
{
    private const string CandidateKey = "/Music/Incoming/Collection/Release 01";

    private static readonly BridgeIdentityPick ReleasePick =
        new BridgeIdentityPick.Release(
            BridgeMetadataSource.MusicBrainz, "rel-picked", BridgeClaimLevel.Exact);

    private static readonly MappingPaneSeedState Untouched =
        new(Seeded: false, Prefetching: false, PrefetchFailed: false);

    [Fact]
    public void AppliesAReadyRowsSettledPick()
    {
        // A settled single match is a pick identification made — the same
        // record a click writes, resumed the same way.
        Assert.Equal(
            ReleasePick,
            PickedResume.From(Row(new BridgeTriagePlacement.Ready(), ReleasePick), Untouched));
    }

    [Fact]
    public void AppliesAChoiceMadeOnANeedsYouRow()
    {
        var placement = new BridgeTriagePlacement.NeedsYou(
            BridgeNeedsYouGroup.PickAPressing,
            new BridgeNeedsYouReason.Disagreement(new BridgeNeedsYou.SeveralMatches(4)));

        Assert.Equal(ReleasePick, PickedResume.From(Row(placement, ReleasePick), Untouched));
    }

    [Fact]
    public void AppliesAStoredUnknownChoice()
    {
        var placement = new BridgeTriagePlacement.NeedsYou(
            BridgeNeedsYouGroup.NoMatch,
            new BridgeNeedsYouReason.Disagreement(new BridgeNeedsYou.NoMatch()));

        Assert.IsType<BridgeIdentityPick.Unknown>(
            PickedResume.From(Row(placement, new BridgeIdentityPick.Unknown()), Untouched));
    }

    [Fact]
    public void IgnoresRowsPastDeciding()
    {
        // Done and Skipped rows keep their pick too, and after a restart their
        // import status is not in the session — placement is the only thing
        // standing between an imported folder and a re-opened commit bar.
        foreach (var placement in new BridgeTriagePlacement[]
        {
            new BridgeTriagePlacement.Done(),
            new BridgeTriagePlacement.Skipped(),
        })
        {
            Assert.Null(PickedResume.From(Row(placement, ReleasePick), Untouched));
        }
    }

    [Fact]
    public void YieldsToAPaneAlreadySeeded()
    {
        Assert.Null(PickedResume.From(
            Row(new BridgeTriagePlacement.Ready(), ReleasePick),
            Untouched with { Seeded = true }));
    }

    [Fact]
    public void YieldsToAReadInFlight()
    {
        Assert.Null(PickedResume.From(
            Row(new BridgeTriagePlacement.Ready(), ReleasePick),
            Untouched with { Prefetching = true }));
    }

    [Fact]
    public void StaysDownAfterAFailedRead()
    {
        // A failed read leaves the pane holding nothing, so without this guard
        // the next queue tick would run the same read again, and on a
        // persistent failure, forever.
        Assert.Null(PickedResume.From(
            Row(new BridgeTriagePlacement.Ready(), ReleasePick),
            Untouched with { PrefetchFailed = true }));
    }

    [Fact]
    public void ARowWithNothingDecidedResumesNothing()
    {
        Assert.Null(PickedResume.From(
            Row(new BridgeTriagePlacement.Ready(), picked: null),
            Untouched));
    }

    [Fact]
    public void AbsentRowResumesNothing()
    {
        Assert.Null(PickedResume.From(row: null, Untouched));
    }

    private static BridgeTriageRow Row(
        BridgeTriagePlacement placement,
        BridgeIdentityPick? picked) => new(
        CandidateKey: CandidateKey,
        FolderName: "Release 01",
        WatchedFolderPath: "/Music/Incoming",
        DisplayPath: "Collection/Release 01",
        ResolvedBoundaries: System.Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
        CombineAncestorKey: null,
        Actionable: true,
        Placement: placement,
        SkipAction: placement switch
        {
            BridgeTriagePlacement.Skipped => BridgeTriageSkipAction.Unskip,
            BridgeTriagePlacement.Done => null,
            _ => BridgeTriageSkipAction.Skip,
        },
        Matched: null,
        Selectable: placement is BridgeTriagePlacement.Ready,
        ImportStatus: null,
        Picked: picked,
        Claim: picked is BridgeIdentityPick.Release release
            ? new BridgeIdentityChoice.Exact(release.ReleaseId, release.Source)
            : null);
}
