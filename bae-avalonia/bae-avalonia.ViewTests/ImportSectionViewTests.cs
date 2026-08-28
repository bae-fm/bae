using System.Reflection;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Avalonia.Media;
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

    // How a folder is read is asked once, on the header of the folder read as
    // several releases — never on the rows it produced, which are releases and
    // not places to answer a question about the folder holding them.
    [AvaloniaFact]
    public void OnlyTheGroupHeaderOffersToReadTheFolderAsOneRelease()
    {
        var view = BuildView(PreviewData.ImportResolvedItems, PreviewData.ImportResolvedSummary);

        Assert.DoesNotContain(
            CandidateRow(view).GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("import.release.one")));
        Assert.NotNull(DecisionButton(view));
    }

    [AvaloniaFact]
    public void FolderDecisionClearsTheSelectedCandidateBeforeDispatch()
    {
        var view = BuildView(PreviewData.ImportResolvedItems, PreviewData.ImportResolvedSummary);
        RaiseTap(CandidateRow(view));
        Assert.Equal(CandidateKey, SelectedKey(view));

        DecisionButton(view).RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Null(SelectedKey(view));
    }

    [AvaloniaFact]
    public void FolderDecisionTapDoesNotActivateACandidate()
    {
        var view = BuildView(PreviewData.ImportResolvedItems, PreviewData.ImportResolvedSummary);

        RaiseTap(DecisionButton(view));

        Assert.Null(SelectedKey(view));
    }

    // Activating a Ready row opens the pane on the release the row settled on.
    [AvaloniaFact]
    public void ActivatingAReadyRowOpensOnItsSettledMatch()
    {
        var placement = new BridgeTriagePlacement.Ready();
        var view = BuildView(
            MatchedItems(placement, BridgeTriageSkipAction.Skip),
            MatchedSummary(placement, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending);

        RaiseTap(CandidateRow(view));

        Assert.Equal(CandidateKey, SelectedKey(view));
    }

    // A failed attempt is Pending work, and the row says what went wrong. It
    // offers no buttons of its own: retrying is the ordinary import, from the
    // pane the row opens like any other.
    [AvaloniaFact]
    public void AFailedRowSaysSoAndOffersNoButtons()
    {
        var placement = new BridgeTriagePlacement.Failed();
        var failure = new BridgeException.Diagnostic(
            new BridgeErrorCategory.Import(), "the disk filled");
        var status = new BridgeTriageImportStatus.Error(failure);
        var view = BuildView(
            MatchedItems(placement, null, status),
            MatchedSummary(placement, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending);

        var row = CandidateRow(view);
        Assert.Contains(
            row.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == Loc.Chrome("import.row.failed"));
        Assert.Contains(
            row.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == BridgeDisplay.LocalizedLine(failure));
        Assert.Empty(row.GetLogicalDescendants().OfType<Button>());
    }

    // A Done row remains available to inspect after its import completes.
    [AvaloniaFact]
    public void ActivatingADoneRowOpensItsCandidate()
    {
        var placement = new BridgeTriagePlacement.Done();
        var view = BuildView(
            MatchedItems(placement, null),
            MatchedSummary(placement, BridgeTriageTab.Done),
            BridgeTriageTab.Done);

        RaiseTap(CandidateRow(view));

        Assert.Equal(CandidateKey, SelectedKey(view));
    }

    // Identify can settle while the folder is already under the pane. The pane
    // keeps the same candidate selected as the row redraws.
    [AvaloniaFact]
    public void AVerdictSettlingUnderTheOpenPaneKeepsItsSelection()
    {
        var identifying = new BridgeTriagePlacement.NeedsYou(
            BridgeNeedsYouGroup.StillIdentifying,
            new BridgeNeedsYouReason.StillIdentifying(BridgeIdentifyPhase.Running));
        var (view, app) = BuildSection(
            MatchedItems(identifying, BridgeTriageSkipAction.Skip),
            MatchedSummary(identifying, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending);
        RaiseTap(CandidateRow(view));

        var ready = new BridgeTriagePlacement.Ready();
        app.ImportStore.SeedPreview(
            MatchedItems(ready, BridgeTriageSkipAction.Skip),
            MatchedSummary(ready, BridgeTriageTab.Pending),
            BridgeTriageTab.Pending);

        Assert.Equal(CandidateKey, SelectedKey(view));
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
        Assert.Contains($"invalid:{PreviewData.ImportRoot}/Broken", tags);

        var badges = view
            .GetLogicalDescendants()
            .OfType<Border>()
            .Where(border => border.CornerRadius.TopLeft > 900)
            .Select(border => ((TextBlock)border.Child!).Text)
            .ToList();
        Assert.Equal(new[] { "4", "2", "3" }, badges);
    }

    // An imported release reads its cloud work off the outbox: while the
    // outbox holds it the row draws the transfer with one indicator — the
    // upload arrow and the bar — and once it is gone the row is done.
    [AvaloniaFact]
    public void CloudImportReactsToOutboxProgress()
    {
        var status = new BridgeTriageImportStatus.Complete(
            "release-a",
            "album-a");
        var done = new BridgeTriagePlacement.Done();
        var (view, app) = BuildSection(
            MatchedItems(done, null, status),
            MatchedSummary(done, BridgeTriageTab.Done),
            BridgeTriageTab.Done);

        var restingRow = CandidateRow(view);
        Assert.DoesNotContain(
            restingRow.GetLogicalDescendants(),
            control => control is ProgressBar);
        Assert.Contains(
            restingRow.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "✓");

        var progress = new BridgeUploadProgress(
            0, 1, 0, 0, 0, 0, 0, 0,
            new BridgeUploadBar(BridgeUploadPhase.Preparing, 5, 20),
            BridgeUploadActivity.Preparing,
            true);
        app.StorageStore.ApplyOutbox(Outbox(7, progress));

        // One indicator: the arrow, and the bar under the line. The row says
        // what the release is, not how many of its files are still waiting.
        var activeRow = CandidateRow(view);
        Assert.Contains(
            activeRow.GetLogicalDescendants().OfType<PathIcon>(),
            icon => icon.Data?.ToString() == Geometry.Parse(Icons.ArrowUp).ToString());
        Assert.DoesNotContain(
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
    }

    // A running import draws its own line off the candidate-runtime signal,
    // and a Reset that does not name its key says nothing is running for it.
    [AvaloniaFact]
    public void AnImportingRowDrawsTheRunItIsToldAbout()
    {
        var importing = new BridgeTriagePlacement.Importing();
        var (view, app) = BuildSection(
            MatchedItems(importing, null, new BridgeTriageImportStatus.Importing()),
            MatchedSummary(importing, BridgeTriageTab.Pending));

        app.ImportStore.ApplyCandidateRuntime(
            new BridgeCandidateRuntimeChange.Updated(
                CandidateKey,
                new BridgeCandidateRuntimeSnapshot(
                    new BridgeIdentifyState.Idle(),
                    new BridgeSignalsToolbar([]),
                    new BridgeImportInFlight(40, null))));

        Assert.Contains(
            CandidateRow(view).GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text is { } line
                && line.Contains("40", StringComparison.Ordinal));

        app.ImportStore.ApplyCandidateRuntime(
            new BridgeCandidateRuntimeChange.Reset([]));

        Assert.Contains(
            CandidateRow(view).GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text is { } line
                && line.Contains("0", StringComparison.Ordinal));
    }

    private static ImportSectionView BuildView(
        IReadOnlyList<BridgeImportListItem> items,
        BridgeImportQueueSummary summary,
        BridgeTriageTab activeTab = BridgeTriageTab.Pending) =>
        BuildSection(items, summary, activeTab).View;

    private static (ImportSectionView View, AppService App) BuildSection(
        IReadOnlyList<BridgeImportListItem> items,
        BridgeImportQueueSummary summary,
        BridgeTriageTab activeTab = BridgeTriageTab.Pending)
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
            IdentifyFolderForLookup = _ => Task.FromResult(true),
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
        BridgeTriageImportStatus? importStatus = null,
        bool isGroupMember = false) => new()
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
                MetadataProvenance: placement
                    is BridgeTriagePlacement.Ready
                        or BridgeTriagePlacement.Done
                    ? new BridgeMetadataProvenance.ExternalRelease(
                        BridgeMetadataSource.MusicBrainz,
                        "rel-matched")
                    : null),
            IsGroupMember: isGroupMember),
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
                    null),
            }
            : Array.Empty<BridgeReadyRowRef>(),
        FirstUnidentified: null);

    // A window carrying one of each item kind, with the group header core emits
    // before the run of rows it holds.
    private static List<BridgeImportListItem> MixedItems()
    {
        var items = new List<BridgeImportListItem>
        {
            new BridgeImportListItem.GroupHeader(
                PreviewData.GroupStableKey(PreviewData.ImportGroupKey),
                new BridgeTriageGroup(PreviewData.ImportGroupKey, "Collection", Combinable: false),
                PreviewData.ImportRoot,
                true,
                1),
        };
        items.AddRange(MatchedItems(
            new BridgeTriagePlacement.Ready(),
            BridgeTriageSkipAction.Skip,
            isGroupMember: true));
        items.Add(new BridgeImportListItem.Invalid(
            $"invalid:{PreviewData.ImportRoot}/Broken",
            new BridgeInvalidCandidate(
                $"{PreviewData.ImportRoot}/Broken",
                "Broken",
                PreviewData.ImportRoot,
                "Broken",
                Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
                new BridgeInvalidReason.NoValidAudio()),
            IsGroupMember: false));
        return items;
    }

    private static BridgeImportQueueSummary MixedSummary() => new(
        Counts: new BridgeTriageTabCounts(Pending: 4, Done: 2, Skipped: 3),
        WatchedFolders: PreviewData.ImportWatchedFolders.ToArray(),
        FolderScanStatuses: Array.Empty<BridgeWatchedFolderScanStatus>(),
        GroupKeys: new[] { PreviewData.ImportGroupKey },
        Ready: Array.Empty<BridgeReadyRowRef>(),
        FirstUnidentified: null);

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

    /// <summary>The one control that reads a folder the other way: the offer
    /// on the header of the folder read as several releases.</summary>
    private static Button DecisionButton(Control root) =>
        root
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
