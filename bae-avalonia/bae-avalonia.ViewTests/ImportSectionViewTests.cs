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
            MatchedQueue(new BridgeTriagePlacement.Ready(), BridgeTriageTab.Pending),
            BridgeTriageTab.Pending,
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
                BridgeTriageTab.Pending),
            BridgeTriageTab.Pending,
            resumed);
        RaiseTap(CandidateRow(view));
        Assert.Empty(resumed);

        app.ImportStore.SeedPreview(
            MatchedQueue(new BridgeTriagePlacement.Ready(), BridgeTriageTab.Pending),
            PreviewData.ImportWatchedFolders,
            BridgeTriageTab.Pending);

        Assert.Equal(new[] { CandidateKey }, resumed);
    }

    // A folder group renders as a header row with its rows as siblings, so
    // there is no Expander indenting them. Collapsing therefore has to be this
    // view's own doing: the header re-renders the list without the group's
    // rows.
    [AvaloniaFact]
    public void CollapsingAFolderGroupHidesItsRows()
    {
        var view = BuildView(PreviewData.ImportQueue);
        var groupedKey = $"{PreviewData.ImportRoot}/Collection/Release 01";
        Assert.Contains(groupedKey, RowTags(view));

        RaiseClick(GroupHeader(view, "Collection"));

        Assert.DoesNotContain(groupedKey, RowTags(view));
        // The ungrouped row is untouched — collapsing hides one group, not the
        // tab.
        Assert.Contains($"{PreviewData.ImportRoot}/Release 03", RowTags(view));
    }

    private static ImportSectionView BuildView(
        BridgeTriageQueue queue,
        BridgeTriageTab activeTab = BridgeTriageTab.Pending,
        List<string>? resumed = null) =>
        BuildSection(queue, activeTab, resumed).View;

    private static (ImportSectionView View, AppService App) BuildSection(
        BridgeTriageQueue queue,
        BridgeTriageTab activeTab = BridgeTriageTab.Pending,
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
            SetFolderReleaseDecision = (_, _) =>
                Task.FromResult((true, (string?)null)),
            // Selecting a candidate reads its mapping before anything is
            // picked, so the pane has a table to show while the release is
            // still the open question.
            CandidateMapping = _ => (
                true,
                ((BridgeMappingTable?)new BridgeMappingTable(
                    Array.Empty<BridgeMappingRow>(), Reconciliation: null), (string?)null)),
            CandidateDecidedIdentity = (key, _) =>
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
            activeTab,
            new[]
            {
                new ImportCandidate
                {
                    Key = CandidateKey,
                    Name = "Release 01",
                    FolderPath = CandidateKey,
                },
            });
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
                                    BridgeMatchedSignal.DiscId)),
                            Selectable: placement is BridgeTriagePlacement.Ready,
                            ImportStatus: null,
                            Picked: placement
                                is BridgeTriagePlacement.Ready
                                    or BridgeTriagePlacement.Done
                                ? new BridgeIdentityPick.Release(
                                    BridgeMetadataSource.MusicBrainz,
                                    "rel-matched",
                                    BridgeClaimLevel.Exact)
                                : null,
                            Claim: placement
                                is BridgeTriagePlacement.Ready
                                    or BridgeTriagePlacement.Done
                                ? new BridgeIdentityChoice.Exact(
                                    "rel-matched",
                                    BridgeMetadataSource.MusicBrainz)
                                : null)),
                }),
        },
        Counts: new BridgeTriageTabCounts(
            Pending: tab is BridgeTriageTab.Pending ? 1u : 0u,
            Done: tab is BridgeTriageTab.Done ? 1u : 0u,
            Skipped: tab is BridgeTriageTab.Skipped ? 1u : 0u),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>());

    private static LibraryService EmptyLibrary() => new()
    {
        SubscribeAlbumPage = (_, _, _, onValue, _) =>
        {
            onValue(Array.Empty<Album>(), 0);
            return new TestSubscription();
        },
        SubscribeComposerPage = (_, _, _, onValue, _) =>
        {
            onValue(Array.Empty<ComposerSummary>(), 0);
            return new TestSubscription();
        },
        SubscribeArtistPage = (_, _, _, onValue, _) =>
        {
            onValue(Array.Empty<ArtistSummary>(), 0);
            return new TestSubscription();
        },
    };

    private sealed class TestSubscription : IDisposable
    {
        public void Dispose() { }
    }

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

    private static void RaiseClick(Button button) =>
        button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static List<string> RowTags(ImportSectionView view) =>
        view
            .GetLogicalDescendants()
            .OfType<Control>()
            .Select(control => control.Tag as string)
            .Where(tag => tag is not null)
            .Select(tag => tag!)
            .ToList();

    private static Button GroupHeader(ImportSectionView view, string name) =>
        view
            .GetLogicalDescendants()
            .OfType<Button>()
            .Single(button => button.GetLogicalDescendants()
                .OfType<TextBlock>()
                .Any(text => text.Text == name));
}
