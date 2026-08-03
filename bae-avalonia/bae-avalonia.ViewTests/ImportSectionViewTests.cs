using System.Reflection;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ImportSectionViewTests
{
    private const string CandidateKey = "/Music/Incoming/Collection/Release 01";

    [AvaloniaFact]
    public void ReadyCheckboxTapDoesNotActivateItsCandidateRow()
    {
        var view = BuildView(PreviewData.ImportQueue);
        var row = CandidateRow(view);
        var checkbox = row
            .GetLogicalDescendants()
            .OfType<Button>()
            .Single(button => button.Content is Border { Width: 18, Height: 18 });

        RaiseTap(checkbox);

        Assert.Null(SelectedKey(view));
    }

    [AvaloniaFact]
    public void FolderDecisionClearsTheSelectedCandidateBeforeDispatch()
    {
        var view = BuildView(PreviewData.ImportResolvedQueue);
        var row = CandidateRow(view);
        RaiseTap(row);
        Assert.Equal(CandidateKey, SelectedKey(view));
        var decision = DecisionButton(row);

        decision.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Null(SelectedKey(view));
    }

    [AvaloniaFact]
    public void FolderDecisionTapDoesNotReactivateItsCandidateRow()
    {
        var view = BuildView(PreviewData.ImportResolvedQueue);
        var row = CandidateRow(view);
        var decision = DecisionButton(row);

        RaiseTap(decision);

        Assert.Null(SelectedKey(view));
    }

    // Activating a Ready row opens the pane on the release the row settled on,
    // with no trip through the search editor.
    [AvaloniaFact]
    public void ActivatingAReadyRowPrefetchesItsSettledMatch()
    {
        var resumed = new List<string>();
        var view = BuildView(
            MatchedQueue(new BridgeTriagePlacement.Ready(), BridgeTriageTab.Ready),
            BridgeTriageTab.Ready,
            resumed);

        RaiseTap(CandidateRow(view));

        Assert.Equal(new[] { CandidateKey }, resumed);
    }

    // A Done row holds a match as well, and re-showing an imported folder must
    // not re-open a pane that can commit it a second time.
    [AvaloniaFact]
    public void ActivatingADoneRowPrefetchesNothing()
    {
        var resumed = new List<string>();
        var view = BuildView(
            MatchedQueue(new BridgeTriagePlacement.Done(), BridgeTriageTab.Done),
            BridgeTriageTab.Done,
            resumed);

        RaiseTap(CandidateRow(view));

        Assert.Empty(resumed);
    }

    // Identify can settle while the folder is already under the pane. The row
    // arrives as a queue read, not as a click, and the pane opens on the match
    // the same way it would have on a click.
    [AvaloniaFact]
    public void AVerdictSettlingToReadyPrefetchesUnderTheOpenPane()
    {
        var resumed = new List<string>();
        var (view, app) = BuildSection(
            MatchedQueue(
                new BridgeTriagePlacement.NeedsYou(
                    BridgeNeedsYouGroup.StillIdentifying,
                    new BridgeNeedsYouReason.StillIdentifying(BridgeIdentifyPhase.Running)),
                BridgeTriageTab.NeedsYou),
            BridgeTriageTab.NeedsYou,
            resumed);
        RaiseTap(CandidateRow(view));
        Assert.Empty(resumed);

        app.ImportStore.SeedPreview(
            MatchedQueue(new BridgeTriagePlacement.Ready(), BridgeTriageTab.Ready),
            PreviewData.ImportWatchedFolders,
            BridgeTriageTab.NeedsYou);

        Assert.Equal(new[] { CandidateKey }, resumed);
    }

    private static ImportSectionView BuildView(
        BridgeTriageQueue queue,
        BridgeTriageTab activeTab = BridgeTriageTab.Ready,
        List<string>? resumed = null) =>
        BuildSection(queue, activeTab, resumed).View;

    private static (ImportSectionView View, AppService App) BuildSection(
        BridgeTriageQueue queue,
        BridgeTriageTab activeTab = BridgeTriageTab.Ready,
        List<string>? resumed = null)
    {
        // Everything below constructs controls, which is only legal on the
        // headless session's dispatcher thread — [AvaloniaFact] is what puts a
        // test body there. Stated here so a test that arrives with a plain
        // [Fact] fails on its first run with the reason, rather than passing
        // whenever xunit happens to hand it the right worker thread.
        Dispatcher.UIThread.VerifyAccess();

        var import = new ImportService
        {
            CandidateForKey = key => (
                true,
                new ImportCandidate { Key = key, Name = "Release 01", FolderPath = key }),
            SetFolderReleaseDecision = (_, _) =>
                Task.FromResult((true, (string?)null)),
            // Selecting a candidate reads its mapping before anything is
            // picked, so the pane has a table to show while the release is
            // still the open question.
            CandidateMapping = _ => (
                true,
                ((BridgeMappingTable?)new BridgeMappingTable(
                    Array.Empty<BridgeMappingRow>(), Reconciliation: null), (string?)null)),
            CandidateDecidedIdentity = key =>
            {
                resumed?.Add(key);
                return Task.FromResult(
                    (true, ((DecidedEdit?)null, Undecided: false, (string?)"stub")));
            },
        };
        var playback = new PlaybackService
        {
            PreviewStop = () => true,
        };
        var app = AppService.Stubbed(
            new SessionStore(Dispatcher.UIThread),
            Dispatcher.UIThread,
            EmptyLibrary(),
            import,
            playback);
        var view = new ImportSectionView(
            app,
            new ImportDialogs(
                new ModalHost(),
                new LightboxOverlay(),
                app.Images,
                _ => Task.CompletedTask));
        app.ImportStore.SeedPreview(
            queue,
            PreviewData.ImportWatchedFolders,
            activeTab);
        return (view, app);
    }

    // One candidate carrying a settled match, under whichever placement the test
    // is asking about — the shape only the placement distinguishes.
    private static BridgeTriageQueue MatchedQueue(
        BridgeTriagePlacement placement,
        BridgeTriageTab tab) => new(
        Sections: new[]
        {
            new BridgeTriageSection(
                tab,
                PreviewData.ImportRoot,
                null,
                new BridgeTriageEntry[]
                {
                    new BridgeTriageEntry.Candidate(
                        CandidateKey,
                        new BridgeTriageRow(
                            CandidateKey: CandidateKey,
                            FolderName: "Release 01",
                            WatchedFolderPath: PreviewData.ImportRoot,
                            DisplayPath: "Collection/Release 01",
                            ResolvedBoundaries: Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
                            CombineAncestorKey: null,
                            Actionable: true,
                            Placement: placement,
                            Matched: new BridgeMatchedRelease(
                                ReleaseId: "rel-matched",
                                Title: "Album Title",
                                Artist: "Artist Name",
                                Pressing: null,
                                CoverThumbnailUrl: null,
                                Evidence: new BridgeMatchEvidence(
                                    BridgeMetadataSource.MusicBrainz,
                                    BridgeMatchedSignal.DiscId),
                                Claim: new BridgeIdentityChoice.Exact(
                                    "rel-matched",
                                    BridgeMetadataSource.MusicBrainz)),
                            Selectable: placement is BridgeTriagePlacement.Ready,
                            ImportStatus: null,
                            Picked: placement
                                is BridgeTriagePlacement.Ready
                                    or BridgeTriagePlacement.Done
                                ? new BridgeIdentityPick.Release(
                                    BridgeMetadataSource.MusicBrainz,
                                    "rel-matched")
                                : null)),
                }),
        },
        Counts: new BridgeTriageTabCounts(
            Ready: tab is BridgeTriageTab.Ready ? 1u : 0u,
            NeedsYou: tab is BridgeTriageTab.NeedsYou ? 1u : 0u,
            Done: tab is BridgeTriageTab.Done ? 1u : 0u,
            Skipped: tab is BridgeTriageTab.Skipped ? 1u : 0u),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>());

    private static LibraryService EmptyLibrary() => new()
    {
        AlbumCount = () => (true, 0L),
        AlbumPage = (_, _, _) => (true, (new List<Album>(), (string?)null)),
        ComposerCount = () => (true, 0L),
        ComposerPage = (_, _, _) => (true, (new List<ComposerSummary>(), (string?)null)),
        ArtistCount = () => (true, 0L),
        ArtistPage = (_, _, _) => (true, (new List<ArtistSummary>(), (string?)null)),
    };

    private static Control CandidateRow(ImportSectionView view) =>
        view
            .GetLogicalDescendants()
            .OfType<Control>()
            .Single(control => Equals(control.Tag, CandidateKey));

    private static Button DecisionButton(Control row) =>
        row
            .GetLogicalDescendants()
            .OfType<Button>()
            .Single(button => Equals(
                button.Content,
                Loc.Chrome("import.release.one")));

    private static string? SelectedKey(ImportSectionView view) =>
        (string?)typeof(ImportSectionView)
            .GetField("_selectedKey", BindingFlags.Instance | BindingFlags.NonPublic)!
            .GetValue(view);

    private static void RaiseTap(Control control) =>
        control.RaiseEvent(new TappedEventArgs(Gestures.TappedEvent, null!));
}
