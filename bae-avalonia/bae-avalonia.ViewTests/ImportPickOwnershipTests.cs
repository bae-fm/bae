using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

using static Bae.Desktop.ViewTests.ImportCandidateFixtures;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// Picking a pressing is a decision about one folder. The store holds it under
/// that folder's key, so the pane looking at another candidate — or at none —
/// neither cancels the read nor lets it land on whatever is on screen when it
/// comes back.
/// </summary>
public sealed class ImportPickOwnershipTests
{
    private static readonly BridgeMetadataProvenance Provenance =
        new BridgeMetadataProvenance.ExternalRelease(
            BridgeMetadataSource.MusicBrainz,
            "rel-1",
            []);

    [Fact]
    public void ThePaneLookingAwayLeavesThePickInFlight()
    {
        var (store, _) = PickingStore();
        Assert.NotNull(store.BeginMetadataApplication(CandidateKey, Provenance));

        // What the pane does when it moves to another candidate.
        store.ClearObservedCandidate();

        Assert.Equal(Provenance, store.PickInFlight(CandidateKey));
    }

    [Fact]
    public void APickThatLandsPutsItsOwnCandidateBackOnTheDraft()
    {
        var (store, _) = PickingStore();
        store.PresentMetadata(CandidateKey, ImportMetadataPresentation.FindOnline);
        var pick = Assert.IsType<ImportMetadataPick>(
            store.BeginMetadataApplication(CandidateKey, Provenance));

        store.MetadataApplicationSucceeded(CandidateKey, pick);

        Assert.Null(store.PickInFlight(CandidateKey));
        Assert.Equal(
            ImportMetadataPresentation.Draft,
            store.Candidate(CandidateKey)!.MetadataPresentation);
    }

    // The read that returns second is the older one: the candidate belongs to
    // the pick that replaced it, and closing the browser is that pick's to do.
    [Fact]
    public void AReplacedPickCannotLand()
    {
        var (store, _) = PickingStore();
        store.PresentMetadata(CandidateKey, ImportMetadataPresentation.FindOnline);
        var first = Assert.IsType<ImportMetadataPick>(
            store.BeginMetadataApplication(CandidateKey, Provenance));
        var second = Assert.IsType<ImportMetadataPick>(
            store.BeginMetadataApplication(
                CandidateKey,
                new BridgeMetadataProvenance.ExternalRelease(
                    BridgeMetadataSource.MusicBrainz,
                    "rel-2",
                    [])));

        store.MetadataApplicationSucceeded(CandidateKey, first);

        Assert.Same(second.Provenance, store.PickInFlight(CandidateKey));
        Assert.Equal(
            ImportMetadataPresentation.FindOnline,
            store.Candidate(CandidateKey)!.MetadataPresentation);
    }

    [Fact]
    public void ARereadOfTheSameFolderLeavesThePickAlone()
    {
        var (store, deliver) = PickingStore();
        Assert.NotNull(store.BeginMetadataApplication(CandidateKey, Provenance));

        deliver(Detail(Provenance));

        Assert.Equal(Provenance, store.PickInFlight(CandidateKey));
    }

    [Fact]
    public void AudioThatChangedUnderThePickCancelsIt()
    {
        var (store, deliver) = PickingStore();
        Assert.NotNull(store.BeginMetadataApplication(CandidateKey, Provenance));

        deliver(Detail(Provenance, audioIdentity: "rescanned-audio"));

        Assert.Null(store.PickInFlight(CandidateKey));
    }

    [Fact]
    public void AFolderThatIsGoneCancelsThePickMadeOnIt()
    {
        var (store, deliver) = PickingStore();
        Assert.NotNull(store.BeginMetadataApplication(CandidateKey, Provenance));

        deliver(null);

        Assert.Null(store.PickInFlight(CandidateKey));
    }

    // A store with the fixture folder read into it, and the way its own
    // per-candidate read answers again.
    private static (ImportStore Store, Action<BridgeImportCandidateDetail?> Deliver)
        PickingStore()
    {
        Action<BridgeImportCandidateDetail?>? deliver = null;
        var import = new ImportService
        {
            ProjectFolderCandidate = NativeBae.ImportCandidateRow,
            SubscribeImportCandidate = (_, onValue, _) =>
            {
                deliver = onValue;
                return new NoSubscription();
            },
        };
        var store = new ImportStore(import, (_, _) => { }, action => action());
        store.ObserveCandidate(CandidateKey);
        Assert.NotNull(deliver);
        deliver!(Detail(Provenance));
        return (store, detail => deliver!(detail));
    }

    private sealed class NoSubscription : IDisposable
    {
        public void Dispose()
        {
        }
    }
}
