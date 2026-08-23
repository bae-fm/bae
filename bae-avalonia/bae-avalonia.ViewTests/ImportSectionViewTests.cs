using System.Reflection;
using Avalonia;
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
        var view = BuildView(PreviewData.ImportItems, PreviewData.ImportSummary);
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
        var view = BuildView(PreviewData.ImportResolvedItems, PreviewData.ImportResolvedSummary);
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
        var view = BuildView(PreviewData.ImportResolvedItems, PreviewData.ImportResolvedSummary);
        var row = CandidateRow(view);
        var decision = DecisionButton(row);

        RaiseTap(decision);

        Assert.Null(SelectedKey(view));
    }

    // Activating a Ready row opens the pane on the release the row settled on,
    // with no trip through the search editor and nothing asked of core: the
    // pick and everything it produced are already stored.
    [AvaloniaFact]
    public void ActivatingAReadyRowOpensOnItsSettledMatch()
    {
        var asked = new List<string>();
        var placement = new BridgeTriagePlacement.Ready();
        var view = BuildView(
            MatchedItems(placement, BridgeTriageSkipAction.Skip),
            MatchedSummary(placement, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending,
            asked);

        RaiseTap(CandidateRow(view));

        Assert.Equal(CandidateKey, SelectedKey(view));
        Assert.Empty(asked);
    }

    // A Done row holds a match as well, and re-showing an imported folder must
    // not decide anything about it again.
    [AvaloniaFact]
    public void ActivatingADoneRowDecidesNothing()
    {
        var asked = new List<string>();
        var placement = new BridgeTriagePlacement.Done();
        var view = BuildView(
            MatchedItems(placement, null),
            MatchedSummary(placement, BridgeTriageTab.Done),
            BridgeTriageTab.Done,
            asked);

        RaiseTap(CandidateRow(view));

        Assert.Empty(asked);
    }

    // Identify can settle while the folder is already under the pane. The row
    // arrives as a queue read, not as a click, and the pane redraws from it
    // without deciding anything on the user's behalf.
    [AvaloniaFact]
    public void AVerdictSettlingUnderTheOpenPaneDecidesNothing()
    {
        var asked = new List<string>();
        var identifying = new BridgeTriagePlacement.NeedsYou(
            BridgeNeedsYouGroup.StillIdentifying,
            new BridgeNeedsYouReason.StillIdentifying(BridgeIdentifyPhase.Running));
        var (view, app) = BuildSection(
            MatchedItems(identifying, BridgeTriageSkipAction.Skip),
            MatchedSummary(identifying, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending,
            asked);
        RaiseTap(CandidateRow(view));

        var ready = new BridgeTriagePlacement.Ready();
        app.ImportStore.SeedPreview(
            MatchedItems(ready, BridgeTriageSkipAction.Skip),
            MatchedSummary(ready, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending);

        Assert.Equal(CandidateKey, SelectedKey(view));
        Assert.Empty(asked);
    }

    // A folder group renders as a header row with its rows as siblings, so
    // there is no Expander indenting them. Collapsing therefore has to be this
    // view's own doing: the header re-renders the list without the group's
    // rows.
    // Which rows a folded group leaves in the list is core's answer, so the
    // header's job is to say the group is folded. The next read comes back
    // without its rows.
    [AvaloniaFact]
    public void CollapsingAFolderGroupAsksCoreToFoldIt()
    {
        var (view, app) = BuildSection(PreviewData.ImportItems, PreviewData.ImportSummary);
        Assert.Empty(app.ImportStore.View.CollapsedGroups);

        RaiseClick(GroupHeader(view, "Collection"));

        Assert.Equal(
            new[] { PreviewData.ImportGroupKey },
            app.ImportStore.View.CollapsedGroups);
    }

    // One window of the list carries every item kind core places into it, and
    // the numbers beside the tabs are the summary's, not a count of what is on
    // screen.
    [AvaloniaFact]
    public void OneWindowRendersEveryItemKindAndTheSummarysCounts()
    {
        var (view, _) = BuildSection(MixedItems(), MixedSummary());

        var tags = RowTags(view);
        Assert.Contains(PreviewData.GroupStableKey(PreviewData.ImportGroupKey), tags);
        Assert.Contains(PreviewData.CandidateStableKey(CandidateKey), tags);
        Assert.Contains(PreviewData.BoundaryStableKey(PreviewData.ImportRoot, "Archive/Box"), tags);
        Assert.Contains($"invalid:{PreviewData.ImportRoot}/Broken", tags);

        var badges = view
            .GetLogicalDescendants()
            .OfType<Border>()
            .Where(border => border.CornerRadius.TopLeft > 900)
            .Select(border => ((TextBlock)border.Child!).Text)
            .ToList();
        Assert.Equal(new[] { "4", "2", "3" }, badges);
    }

    [AvaloniaFact]
    public void CloudImportReactsToOutboxProgressAndTerminalRevision()
    {
        var status = new BridgeTriageImportStatus.CloudUploadQueued(
            "release-a",
            "album-a",
            7);
        var done = new BridgeTriagePlacement.Done();
        var (view, app) = BuildSection(
            MatchedItems(done, null, status),
            MatchedSummary(done, BridgeTriageTab.Done),
            BridgeTriageTab.Done);

        var queuedRow = CandidateRow(view);
        Assert.Contains(
            queuedRow.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Core("core.queue.queued", "count", 1));
        Assert.True(
            queuedRow.GetLogicalDescendants().OfType<ProgressBar>().Single()
                .IsIndeterminate);

        var progress = new BridgeUploadProgress(
            0, 1, 0, 0, 0, 0, 0, 0,
            new BridgeUploadBar(BridgeUploadPhase.Preparing, 5, 20),
            BridgeUploadActivity.Preparing,
            true);
        app.StorageStore.ApplyOutbox(Outbox(7, progress));

        var activeRow = CandidateRow(view);
        Assert.Contains(
            activeRow.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text is { } line
                && line.Contains(
                    Loc.Core("core.outbox.preparing", "count", 1),
                    StringComparison.Ordinal));
        var activeBar = activeRow
            .GetLogicalDescendants()
            .OfType<ProgressBar>()
            .Single();
        Assert.False(activeBar.IsIndeterminate);
        Assert.Equal(0.25, activeBar.Value);

        app.StorageStore.ApplyOutbox(Outbox(8));

        var finishedRow = CandidateRow(view);
        Assert.DoesNotContain(
            finishedRow.GetLogicalDescendants(),
            control => control is ProgressBar);
        Assert.Contains(
            finishedRow.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "✓");

        app.StorageStore.ApplyOutbox(Outbox(6, progress));

        var afterOlderSnapshot = CandidateRow(view);
        Assert.DoesNotContain(
            afterOlderSnapshot.GetLogicalDescendants(),
            control => control is ProgressBar);
        Assert.Contains(
            afterOlderSnapshot.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "✓");
    }

    private static ImportSectionView BuildView(
        IReadOnlyList<BridgeImportListItem> items,
        BridgeImportQueueSummary summary,
        BridgeTriageTab activeTab = BridgeTriageTab.Pending,
        List<string>? resumed = null) =>
        BuildSection(items, summary, activeTab, resumed).View;

    private static (ImportSectionView View, AppService App) BuildSection(
        IReadOnlyList<BridgeImportListItem> items,
        BridgeImportQueueSummary summary,
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
            // The pane's candidate is seeded below; its own query stays open
            // and silent, so the seeded value is what the pane reads.
            SubscribeImportCandidate = (_, _, _) => new TestSubscription(),
            // Nothing about selecting a row decides an identity: what a pick
            // produced is already stored. A test that sees a call here has
            // found the pane deciding on the user's behalf.
            PickCandidateIdentity = (key, _) =>
            {
                resumed?.Add(key);
                return Task.FromResult((true, (string?)null));
            },
            AutoIdentifyFolder = _ => Task.FromResult(true),
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
            items,
            summary,
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
        Realize(view);
        return (view, app);
    }

    // The list virtualizes, so its rows exist only once something has laid it
    // out. A headless window is what does that here; without it the panel
    // realizes nothing and every row lookup below finds nothing.
    private static void Realize(Control view)
    {
        var window = new Window { Width = 900, Height = 700, Content = view };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        window.Measure(new Size(900, 700));
        window.Arrange(new Rect(0, 0, 900, 700));
        Dispatcher.UIThread.RunJobs();
    }

    // One candidate carrying a settled match, under whichever placement the test
    // is asking about — the shape only the placement distinguishes.
    private static List<BridgeImportListItem> MatchedItems(
        BridgeTriagePlacement placement,
        BridgeTriageSkipAction? skipAction,
        BridgeTriageImportStatus? importStatus = null) => new()
    {
        new BridgeImportListItem.Candidate(
            PreviewData.CandidateStableKey(CandidateKey),
            new BridgeTriageRow(
                CandidateKey: CandidateKey,
                FolderName: "Release 01",
                WatchedFolderPath: PreviewData.ImportRoot,
                DisplayPath: "Collection/Release 01",
                ResolvedBoundaries: Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
                CombineAncestorKey: null,
                Actionable: true,
                Placement: placement,
                SkipAction: skipAction,
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
                ImportStatus: importStatus,
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
    };

    private static BridgeImportQueueSummary MatchedSummary(
        BridgeTriagePlacement placement,
        BridgeTriageTab tab) => new(
        Counts: new BridgeTriageTabCounts(
            Pending: tab is BridgeTriageTab.Pending ? 1u : 0u,
            Done: tab is BridgeTriageTab.Done ? 1u : 0u,
            Skipped: tab is BridgeTriageTab.Skipped ? 1u : 0u),
        WatchedFolders: PreviewData.ImportWatchedFolders.ToArray(),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>(),
        GroupKeys: Array.Empty<BridgeFolderReleaseDecisionKey>(),
        Ready: placement is BridgeTriagePlacement.Ready
            ? new[]
            {
                new BridgeReadyRowRef(
                    CandidateKey,
                    new BridgeIdentityChoice.Exact("rel-matched", BridgeMetadataSource.MusicBrainz),
                    null),
            }
            : Array.Empty<BridgeReadyRowRef>(),
        FirstUnidentifiedKey: null);

    // A window carrying one of each item kind, with the group header core emits
    // before the run of rows it holds.
    private static List<BridgeImportListItem> MixedItems()
    {
        var items = new List<BridgeImportListItem>
        {
            new BridgeImportListItem.GroupHeader(
                PreviewData.GroupStableKey(PreviewData.ImportGroupKey),
                new BridgeTriageGroup(PreviewData.ImportGroupKey, "Collection"),
                PreviewData.ImportRoot,
                true,
                1),
        };
        items.AddRange(MatchedItems(new BridgeTriagePlacement.Ready(), BridgeTriageSkipAction.Skip));
        items.Add(new BridgeImportListItem.Boundary(
            PreviewData.BoundaryStableKey(PreviewData.ImportRoot, "Archive/Box"),
            new BridgeFolderReleaseBoundary(
                new BridgeFolderReleaseDecisionKey(PreviewData.ImportRoot, "Archive/Box"),
                "Box",
                "Archive/Box",
                2,
                Array.Empty<BridgeFolderReleaseTreeRow>())));
        items.Add(new BridgeImportListItem.Invalid(
            $"invalid:{PreviewData.ImportRoot}/Broken",
            new BridgeInvalidCandidate(
                $"{PreviewData.ImportRoot}/Broken",
                "Broken",
                PreviewData.ImportRoot,
                "Broken",
                Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
                new BridgeInvalidReason.NoValidAudio())));
        return items;
    }

    private static BridgeImportQueueSummary MixedSummary() => new(
        Counts: new BridgeTriageTabCounts(Pending: 4, Done: 2, Skipped: 3),
        WatchedFolders: PreviewData.ImportWatchedFolders.ToArray(),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>(),
        GroupKeys: new[] { PreviewData.ImportGroupKey },
        Ready: Array.Empty<BridgeReadyRowRef>(),
        FirstUnidentifiedKey: null);

    private static BridgeOutboxSnapshot Outbox(
        ulong revision,
        BridgeUploadProgress? progress = null) => new(
        revision,
        [],
        [],
        progress is null
            ? new Dictionary<string, BridgeUploadProgress>()
            : new Dictionary<string, BridgeUploadProgress>
            {
                ["release-a"] = progress,
            },
        new BridgeUploadProgress(
            0, 0, 0, 0, 0, 0, 0, 0, null, null, false),
        0,
        [],
        BridgeOutboxPauseState.Running,
        0,
        null);

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
            .Single(control => Equals(
                control.Tag,
                PreviewData.CandidateStableKey(CandidateKey)));

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
