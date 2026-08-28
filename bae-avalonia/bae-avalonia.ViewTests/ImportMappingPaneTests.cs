using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
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

/// <summary>
/// The pane, over one candidate's stored detail. Everything it shows — the
/// picked release, the form with what was typed over it, the table, the cover,
/// the last failed import — arrives in that one value, so showing a candidate
/// is a read and not a sequence of them. These check what the pane makes of it,
/// and that its controls write back rather than keeping a copy.
/// </summary>
public sealed class ImportMappingPaneTests
{
    private const string CandidateKey = "/Music/Incoming/Album";

    // Opening a candidate draws the whole pane at once: the header states the
    // release as the stored edit has it, the table lists the folder's units,
    // and the cover is the one the candidate stores.
    [AvaloniaFact]
    public void ThePaneDrawsTheStoredFormTableAndCover()
    {
        var (pane, _) = Show(Detail());

        // The header states the release as the stored edit has it.
        Assert.Contains("Typed Over The Release", Texts(pane));
        // The form and the table are editable, so their values are the boxes'.
        var fields = Fields(pane);
        Assert.Contains("1996", fields);
        Assert.Contains("Track One", fields);
        Assert.Contains("Track Two", fields);
        // The card — cover tile included — is drawn because something is
        // picked; an undecided folder has no cover to show and no card to
        // show it in.
        Assert.Single(
            pane.GetLogicalDescendants().OfType<Image>(),
            image => image.Width == ImportMetadataSourceSection.CoverSize);
    }

    // The pane leads with the folder it is about — the one fact nothing below
    // it can change — with the audio it holds beside the name.
    [AvaloniaFact]
    public void ThePaneLeadsWithTheFolderItIsAbout()
    {
        var (pane, _) = Show(Detail());

        Assert.Contains("Album", Texts(pane));
        Assert.Contains("FLAC", Texts(pane));
    }

    [AvaloniaFact]
    public void SelectingACandidateDoesNotStartLookup()
    {
        var identified = new List<string>();

        Show(Detail(), identified: identified);

        Assert.Empty(identified);
    }

    [AvaloniaFact]
    public void ExplicitLookupStartsLookupWithoutReplacingTheStoredSeed()
    {
        var identified = new List<string>();
        var history = new List<BridgeImportMetadataMode>();
        var detail = Detail(new BridgeMetadataSeed.FileTags());
        var (pane, _) = Show(
            detail,
            identified: identified,
            modeHistory: history);

        pane.GetLogicalDescendants().OfType<Button>()
            .First(button => Equals(
                button.Content,
                Loc.Core("ui.import.metadata.lookup")))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Equal(new[] { CandidateKey }, identified);
        Assert.Equal(new[] { BridgeImportMetadataMode.Lookup }, history);
        Assert.IsType<BridgeMetadataSeed.FileTags>(detail.Row.MetadataSeed);
    }

    [AvaloniaFact]
    public void ManualMetadataIsNotLabeledAsFileTags()
    {
        var (pane, _) = Show(Detail(new BridgeMetadataSeed.Manual()));

        Assert.Contains(Loc.Core("ui.import.metadata.manual"), Texts(pane));
        Assert.DoesNotContain(
            Loc.Core("ui.import.metadata.from_file_tags"),
            Texts(pane));
    }

    [AvaloniaFact]
    public void ManualBeforeSelectionShowsBlankProjectedMetadata()
    {
        var (pane, _) = Show(Detail(metadataSeed: null));

        Click(pane, Loc.Core("ui.import.metadata.manual"));

        Assert.Contains(
            pane.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.FontSize == 16
                && text.Text == Loc.Core("ui.import.slots.untitled"));
    }

    [AvaloniaFact]
    public void FileTagsAndManualAreSelectedOnlyByTheirAffirmativeActions()
    {
        var seeds = new List<BridgeMetadataSeed>();
        var (fileTagsPane, _) = Show(
            Detail(metadataSeed: null),
            selectedSeeds: seeds);

        Click(fileTagsPane, Loc.Core("ui.import.metadata.file_tags"));
        Click(fileTagsPane, Loc.Chrome("import.metadata.use_file_tags"));

        var (manualPane, _) = Show(
            Detail(metadataSeed: null),
            selectedSeeds: seeds);
        Click(manualPane, Loc.Core("ui.import.metadata.manual"));
        Click(manualPane, Loc.Chrome("import.metadata.enter_manually"));

        Assert.Collection(
            seeds,
            seed => Assert.IsType<BridgeMetadataSeed.FileTags>(seed),
            seed => Assert.IsType<BridgeMetadataSeed.Manual>(seed));
    }

    // A failure that outlived the process is on the pane with the one action
    // that answers it. Both halves matter: the error says what happened, and
    // Retry is how the person acts on it without hunting for the commit bar.
    [AvaloniaFact]
    public void AStoredFailureShowsItsErrorAndOffersRetry()
    {
        var (pane, _) = Show(Detail(failure: new BridgeImportFailure(
            Error: "the disk filled",
            FailedAt: "2026-01-15T12:00:00Z")));

        Assert.Contains("the disk filled", Texts(pane));
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("import.row.retry")));
    }

    // With no failure stored there is nothing to answer, so neither half is
    // drawn — the banner is the stored row, not a slot the pane always keeps.
    [AvaloniaFact]
    public void WithNothingFailedThereIsNoBanner()
    {
        var (pane, _) = Show(Detail());

        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("import.row.retry")));
    }

    // Typing in the form writes the field to core. The pane keeps no copy: the
    // value it drew came from the detail, and the next detail is what redraws
    // it — so a test that sees no call has found the pane editing its own copy.
    [AvaloniaFact]
    public void LeavingAnEditedFieldWritesItToCore()
    {
        var written = new List<(string Key, BridgeCandidateEditField Field, string Value)>();
        var (pane, _) = Show(Detail(), onEditField: (key, field, value) =>
            written.Add((key, field, value)));

        var year = pane.GetLogicalDescendants().OfType<TextBox>()
            .First(box => box.Text == "1996");
        year.Text = "2011";
        year.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Equal(
            new[] { (CandidateKey, BridgeCandidateEditField.Year, "2011") },
            written);
    }

    // While the import runs, what the card offers is not a button: the commit
    // already happened, and the question is how far along it is. The card says
    // the same step, percent and bar the candidate's row says, from the one
    // component that reads that run.
    [AvaloniaFact]
    public void TheCardShowsTheRunningImportWhereTheImportButtonWas()
    {
        var (pane, _) = Show(
            Detail(),
            running: new BridgeCandidateRuntimeSnapshot(
                new BridgeIdentifyState.Idle(),
                new BridgeSignalsToolbar(Array.Empty<BridgeToolbarSignal>()),
                new BridgeImportInFlight(37, new BridgeImportStep.Running(
                    BridgeImportPhase.ReadingFiles))));

        var texts = pane.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();
        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("action.import")));
        Assert.Contains(texts, text => text.Contains("37", StringComparison.Ordinal));
    }

    // ── Building the pane ────────────────────────────────────────────────────

    private static (ImportMappingPane Pane, AppService App) Show(
        BridgeImportCandidateDetail detail,
        Action<string, BridgeCandidateEditField, string>? onEditField = null,
        BridgeCandidateRuntimeSnapshot? running = null,
        List<string>? identified = null,
        List<BridgeImportMetadataMode>? modeHistory = null,
        List<BridgeMetadataSeed>? selectedSeeds = null)
    {
        // Controls may only be built on the headless session's dispatcher
        // thread, which [AvaloniaFact] is what supplies.
        Dispatcher.UIThread.VerifyAccess();

        var import = new ImportService
        {
            // The pane's candidate is seeded below and its own query stays
            // silent, so what the pane reads is exactly the detail handed in.
            SubscribeImportCandidate = (_, _, _) => new NoSubscription(),
            // The picked release's library membership is a separate live read;
            // it stays silent so the banner it drives never appears.
            SubscribeReleaseLibraryStatus = (_, _, _, _, _) => new NoSubscription(),
            SetCandidateEditField = (key, field, value) =>
            {
                onEditField?.Invoke(key, field, value);
                return Task.FromResult((true, (string?)null));
            },
            AutoIdentifyFolder = key =>
            {
                identified?.Add(key);
                return Task.FromResult(true);
            },
            PreviewFileTags = _ => Task.FromResult((
                true,
                ((BridgeReleaseUserEdit?)FileTagsEdit(), (string?)null))),
            SelectCandidateMetadataSeed = (_, seed) =>
            {
                selectedSeeds?.Add(seed);
                return Task.FromResult((true, (string?)null));
            },
            // The run the pane and the progress line both read. Absent leaves
            // the candidate at rest, where the card offers the Import button.
            CandidateRuntime = _ => running,
        };
        var app = AppService.Stubbed(
            new SessionStore(Dispatcher.UIThread),
            Dispatcher.UIThread,
            new LibraryService(),
            import,
            new PlaybackService { PreviewStop = () => true },
            new SettingsService
            {
                GetSettings = () => (true, new Settings()),
                SetLastImportMetadataMode = mode =>
                {
                    modeHistory?.Add(mode);
                    return (true, null);
                },
            });
        var candidate = new ImportCandidate
        {
            Key = CandidateKey,
            Name = "Album",
            FolderPath = CandidateKey,
            Files = detail.Candidate.Files,
            Detail = detail,
        };
        candidate.ResolvePresentedMetadataMode(BridgeImportMetadataMode.Lookup);
        app.ImportStore.SeedPreview(
            Array.Empty<BridgeImportListItem>(),
            PreviewData.ImportSummary,
            BridgeTriageTab.Pending,
            new[]
            {
                candidate,
            });
        var pane = new ImportMappingPane(
            app,
            new ImportDialogs(
                new ModalHost(),
                new LightboxOverlay(),
                app.Images,
                _ => Task.CompletedTask));
        // Showing a candidate renders more than once — clearing the previous
        // release's library-status watch renders, and ShowCandidate renders
        // again after it — so every test here also exercises a rebuild.
        pane.ShowCandidate(detail.Row).GetAwaiter().GetResult();
        Realize(pane);
        return (pane, app);
    }

    // Nothing here virtualizes, but the pane still needs a laid-out tree for
    // its rows to exist.
    private static void Realize(Control pane)
    {
        var window = new Window { Width = 900, Height = 700, Content = pane };
        window.Show();
        Dispatcher.UIThread.RunJobs();
        window.Measure(new Size(900, 700));
        window.Arrange(new Rect(0, 0, 900, 700));
        Dispatcher.UIThread.RunJobs();
    }

    // ── The stored detail ────────────────────────────────────────────────────

    /// <summary>One folder picked as a release, with a title and a year typed
    /// over what the release states, two measured tracks, and a chosen cover.
    /// </summary>
    private static BridgeImportCandidateDetail Detail(BridgeImportFailure? failure = null) =>
        Detail(
            new BridgeMetadataSeed.ExternalRelease(
                BridgeMetadataSource.MusicBrainz,
                "rel-1"),
            failure);

    private static BridgeImportCandidateDetail Detail(
        BridgeMetadataSeed? metadataSeed,
        BridgeImportFailure? failure = null) =>
        new(
            Candidate: new BridgeFolderCandidate(
                FolderPath: CandidateKey,
                SourceFolderName: "Album",
                WatchedFolderPath: "/Music/Incoming",
                Files: new BridgeCandidateFiles(
                    Array.Empty<BridgeCandidateFile>(),
                    "FLAC",
                    Array.Empty<BridgeCollapsedDirectory>()),
                TrackCount: 2,
                Skipped: false,
                IsAdded: false),
            Actionable: true,
            ResumedIdentifyState: new BridgeIdentifyState.Idle(),
            Row: Row(metadataSeed),
            Release: null,
            PickedLibraryStatus: null,
            FileEvidence: Array.Empty<BridgeFileEvidence>(),
            Edit: metadataSeed is null
                ? null
                : new BridgeRawReleaseEdit(
                    "Typed Over The Release",
                    ArtistAssignments(),
                    new BridgeRawPressingEdit(
                        "1996", "CD", "Label Name", "CAT-1", "UK", string.Empty),
                    Array.Empty<BridgeRawTrackEdit>()),
            Mapping: new BridgeMappingTable(
                Array.Empty<BridgeMappingImage>(),
                new[] { TrackRow("01.flac", "Track One"), TrackRow("02.flac", "Track Two") },
                Reconciliation: null),
            Unprobed: Array.Empty<BridgeAudioFile>(),
            Cover: new BridgeCoverChoice(
                new BridgeCoverSelection.ReleaseImage("cover.jpg"),
                new BridgeCoverImageSource.Local("/Music/Incoming/Album/cover.jpg"),
                new BridgeCoverImageSource.Local("/Music/Incoming/Album/cover.jpg")),
            Signals: null,
            Failure: failure);

    private static BridgeTriageRow Row(BridgeMetadataSeed? metadataSeed) => new(
        CandidateKey: CandidateKey,
        FolderName: "Album",
        WatchedFolderPath: "/Music/Incoming",
        DisplayPath: "Album",
        ResolvedBoundaries: Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
        CombineAncestorKey: null,
        Actionable: true,
        Placement: new BridgeTriagePlacement.Ready(),
        SkipAction: BridgeTriageSkipAction.Skip,
        Matched: null,
        Selectable: true,
        ImportStatus: null,
        MetadataSeed: metadataSeed);

    private static BridgeMappingRow TrackRow(string fileId, string title) =>
        new BridgeMappingRow.Unit(new BridgeMappingUnit(
            new BridgeMappingSource.File(new BridgeMappingFile(
                FileId: fileId,
                Name: fileId,
                Size: 1024,
                LocalPath: $"/Music/Incoming/Album/{fileId}",
                ProbedDurationMs: 180_000,
                Role: BridgeMappingRole.Audio,
                Alternatives: Array.Empty<BridgeFileRoleChoice>(),
                RoleChoice: null)),
            new BridgeMappingBecomes.Track(
                new BridgeRawTrackEdit(
                    fileId,
                    title,
                    new BridgeTrackArtistAssignments.Explicit(ArtistAssignments()),
                    1,
                    null,
                    new BridgeAudioFile.Standalone(fileId)),
                SourcePosition: null,
                SourceDurationMs: null)));

    private static BridgeArtistAssignment[] ArtistAssignments() =>
    [
        new BridgeArtistAssignment.New(
            new BridgeNewArtistSeed("Artist Name", null, null, null)),
    ];

    private static BridgeReleaseUserEdit FileTagsEdit() => new(
        "Album Title",
        ArtistAssignments(),
        new BridgePressingEdit(1996, "CD", "Label Name", "CAT-1", "UK", null),
        Array.Empty<BridgeTrackUserEdit>());

    private static void Click(Control pane, string label) =>
        Assert.Single(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, label))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static IReadOnlyList<string> Fields(Control pane) =>
        pane.GetLogicalDescendants().OfType<TextBox>()
            .Select(box => box.Text ?? string.Empty).ToList();

    private static IReadOnlyList<string> Texts(Control pane) =>
        pane.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();

    private sealed class NoSubscription : IDisposable
    {
        public void Dispose() { }
    }
}
