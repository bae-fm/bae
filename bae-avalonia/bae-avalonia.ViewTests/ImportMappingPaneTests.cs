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
    private static readonly BridgeAudioFormat SourceAudio = new(
        Codec: "FLAC",
        SampleRateHz: 44_100,
        BitsPerSample: 16,
        BitrateKbps: null,
        Channels: 2);

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

    [AvaloniaFact]
    public void CandidateMappingRemainsVisibleBeforeMetadataIsAffirmed()
    {
        var (pane, _) = Show(Detail(
            metadataProvenance: null,
            edit: BlankEdit()));

        Assert.Contains("Track One", Fields(pane));
        Assert.Contains("Track Two", Fields(pane));
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(
                button.Content,
                Loc.Chrome("import.metadata.find_online_ellipsis")));
        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("action.import")));
    }

    [AvaloniaFact]
    public void MetadataCardDoesNotRepeatASectionHeading()
    {
        var (pane, _) = Show(Detail(
            metadataProvenance: null,
            edit: BlankEdit()));

        Assert.DoesNotContain(
            Loc.Core("ui.import.metadata.title"),
            Texts(pane));
    }

    // The pane leads with the folder it is about — the one fact nothing below
    // it can change — and includes the source audio in its metadata section.
    [AvaloniaFact]
    public void ThePaneLeadsWithTheFolderItIsAbout()
    {
        var (pane, _) = Show(Detail());

        Assert.Contains("Album", Texts(pane));
        Assert.Contains(Texts(pane), text =>
            text.StartsWith("FLAC", StringComparison.Ordinal));
    }

    [AvaloniaFact]
    public void SelectingACandidateDoesNotStartLookup()
    {
        var identified = new List<string>();

        Show(Detail(), identified: identified);

        Assert.Empty(identified);
    }

    [AvaloniaFact]
    public void OpeningFindOnlineStartsIdentificationWhenEnabled()
    {
        var identified = new List<string>();
        var detail = Detail(
            metadataProvenance: null,
            edit: BlankEdit());
        var (pane, _) = Show(
            detail,
            identified: identified,
            identifyAutomatically: true);

        pane.GetLogicalDescendants().OfType<Button>()
            .First(button => Equals(
                button.Content,
                Loc.Chrome("import.metadata.find_online_ellipsis")))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Equal(new[] { CandidateKey }, identified);
        Assert.Null(detail.MetadataProvenance);
    }

    // Find online is one page. The setting says only whether identification
    // starts on its own; the typed form is there either way.
    [AvaloniaFact]
    public void OpeningFindOnlineShowsTheFormWithoutStartingIdentification()
    {
        var identified = new List<string>();
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            identified: identified,
            identifyAutomatically: false);

        Click(pane, Loc.Chrome("import.metadata.find_online_ellipsis"));

        Assert.Empty(identified);
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("action.search")));
    }

    [AvaloniaFact]
    public void AutomaticMethodShowsSignalBadgesAndRunAgain()
    {
        var runtime = new BridgeCandidateRuntimeSnapshot(
            new BridgeIdentifyState.NotFoundAnywhere(),
            new BridgeSignalsToolbar(new[]
            {
                new BridgeToolbarSignal(
                    BridgeSignalKind.Barcode,
                    "0123456789012",
                    BridgeSignalOrigin.Artwork,
                    new BridgeSignalState.NoMatch(),
                    false,
                    Array.Empty<BridgeSignalOption>()),
            }),
            null,
            null);
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            running: runtime,
            initialPresentation: ImportMetadataPresentation.FindOnline);

        Assert.Contains(Loc.Chrome("signal.kind.barcode"), Texts(pane));
        Assert.Contains("0123456789012", Texts(pane));
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(
                ToolTip.GetTip(button),
                Loc.Chrome("import.rerun_identify")));
    }

    [AvaloniaFact]
    public void TheSearchFormStartsBlankAndKeepsWhatIsTypedAcrossQueryTypes()
    {
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            identifyAutomatically: false);

        Click(pane, Loc.Chrome("import.metadata.find_online_ellipsis"));
        Assert.DoesNotContain("Album", Fields(pane));

        var artist = FieldByLabel(
            pane,
            Loc.Chrome("import.field.artist_manual"));
        artist.Text = "Typed artist";
        Dispatcher.UIThread.RunJobs();
        Assert.Equal("Typed artist", artist.Text);

        Click(pane, Loc.Chrome("signal.kind.catalog"));
        Assert.DoesNotContain("Typed artist", Fields(pane));
        Click(pane, Loc.Chrome("import.search.general"));

        Assert.Contains("Typed artist", Fields(pane));
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("signal.kind.catalog")));
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("signal.kind.barcode")));
    }

    // Every configured provider answers a person's search, so the form offers
    // no source selection to get wrong.
    [AvaloniaFact]
    public void TheSearchFormOffersNoSourceSelection()
    {
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            identifyAutomatically: false);

        Click(pane, Loc.Chrome("import.metadata.find_online_ellipsis"));

        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<CheckBox>(),
            check => Equals(check.Content, "MusicBrainz") || Equals(check.Content, "Discogs"));
        Assert.True(
            Assert.Single(
                pane.GetLogicalDescendants().OfType<Button>(),
                button => Equals(button.Content, Loc.Chrome("action.search")))
            .IsEnabled);
    }

    [AvaloniaFact]
    public void ManualSearchDispatchesTheSelectedQueryType()
    {
        var searches = new List<(string Key, BridgeSearchQuery Query)>();
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            identifyAutomatically: false,
            searches: searches);

        Click(pane, Loc.Chrome("import.metadata.find_online_ellipsis"));
        FieldByLabel(pane, Loc.Chrome("import.field.artist_manual")).Text = "Typed artist";
        FieldByLabel(pane, Loc.Chrome("search.field.album")).Text = "Typed album";
        Dispatcher.UIThread.RunJobs();
        Click(pane, Loc.Chrome("action.search"));
        Dispatcher.UIThread.RunJobs();

        Click(pane, Loc.Chrome("signal.kind.catalog"));
        FieldByLabel(pane, Loc.Chrome("signal.kind.catalog")).Text = "CAT-1";
        Dispatcher.UIThread.RunJobs();
        Click(pane, Loc.Chrome("action.search"));
        Dispatcher.UIThread.RunJobs();

        Click(pane, Loc.Chrome("signal.kind.barcode"));
        FieldByLabel(pane, Loc.Chrome("signal.kind.barcode")).Text = "0123456789012";
        Dispatcher.UIThread.RunJobs();
        Click(pane, Loc.Chrome("action.search"));
        Dispatcher.UIThread.RunJobs();

        Assert.Collection(
            searches,
            search => Assert.Equal(
                (CandidateKey, (BridgeSearchQuery)new BridgeSearchQuery.General(
                    "Typed artist",
                    "Typed album")),
                search),
            search => Assert.Equal(
                (CandidateKey, (BridgeSearchQuery)new BridgeSearchQuery.CatalogNumber("CAT-1")),
                search),
            search => Assert.Equal(
                (CandidateKey,
                    (BridgeSearchQuery)new BridgeSearchQuery.Barcode("0123456789012")),
                search));
    }

    [AvaloniaFact]
    public void AFailedSearchDoesNotClaimToHaveFoundNoMatches()
    {
        var search = new BridgeCandidateSearch(
            new BridgeSearchQuery.General("Artist Name", "Album Title"),
            new BridgeSourceSearch.Failed(new BridgeLookupFailure.Network()),
            new BridgeSourceSearch.NotConfigured(),
            Array.Empty<BridgeReleaseGroup>(),
            new Dictionary<string, BridgeLibraryStatus>(),
            true, false);
        var runtime = new BridgeCandidateRuntimeSnapshot(
            new BridgeIdentifyState.NotFoundAnywhere(),
            new BridgeSignalsToolbar(Array.Empty<BridgeToolbarSignal>()),
            null, search);
        var (pane, _) = Show(Detail(), running: runtime,
            initialPresentation: ImportMetadataPresentation.FindOnline);

        Assert.DoesNotContain(Loc.Chrome("search.no_matches"), Texts(pane));
        Assert.Contains(Loc.Chrome("import.search.source_not_configured", "source", "Discogs"), Texts(pane));
    }

    [AvaloniaFact]
    public void ApplyingAnExistingReleaseWaitsForItsNewDetailRevision()
    {
        var detailCallbacks = new List<Action<BridgeImportCandidateDetail?>>();
        var provenance = new BridgeMetadataProvenance.ExternalRelease(
            BridgeMetadataSource.MusicBrainz,
            "rel-1",
            []);
        var group = ChoiceGroup("rel-1");
        var runtime = new BridgeCandidateRuntimeSnapshot(
            new BridgeIdentifyState.Found(
                new[] { group },
                new Dictionary<string, BridgeLibraryStatus>(),
                1,
                new Dictionary<string, BridgeResultProvenance>()),
            new BridgeSignalsToolbar(Array.Empty<BridgeToolbarSignal>()),
            null,
            null);
        var (pane, _) = Show(
            Detail(provenance, metadataRevision: 1),
            running: runtime,
            initialPresentation: ImportMetadataPresentation.FindOnline,
            applicationRevision: 2,
            detailCallbacks: detailCallbacks);

        Assert.Single(detailCallbacks);
        var choices = Assert.Single(
            pane.GetLogicalDescendants().OfType<ListBox>());
        choices.SelectedIndex = 0;
        Dispatcher.UIThread.RunJobs();

        Assert.Single(
            pane.GetLogicalDescendants().OfType<Spinner>(),
            spinner => spinner.IsVisible);
        detailCallbacks[0](Detail(provenance, metadataRevision: 1));
        Dispatcher.UIThread.RunJobs();
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<ListBox>(),
            list => list.Items.Count == 1);

        detailCallbacks[0](Detail(provenance, metadataRevision: 2));
        Dispatcher.UIThread.RunJobs();
        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<ListBox>(),
            list => list.Items.Count == 1);
        Assert.Contains("Typed Over The Release", Texts(pane));
    }

    // A failure that outlived the process is on the pane with the one action
    // that answers it. Both halves matter: the error says what happened, and
    // Retry is how the person acts on it without hunting for the commit bar.
    [AvaloniaFact]
    public void AStoredFailureShowsItsErrorAndOffersRetry()
    {
        var (pane, _) = Show(Detail(failure: new BridgeImportFailure(
            Error: new BridgeException.Diagnostic(
                new BridgeErrorCategory.Import(),
                "the disk filled"),
            ArtistIdentityConflict: null)));

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
            new[] { (CandidateKey, BridgeCandidateEditField.PressingYear, "2011") },
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
                    BridgeImportPhase.ReadingFiles)),
                null));

        var texts = pane.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();
        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("action.import")));
        Assert.Contains(texts, text => text.Contains("37", StringComparison.Ordinal));
    }

    [AvaloniaFact]
    public void ImportingUsesTheReadOnlySourcePane()
    {
        var (pane, _) = Show(Detail(
            importStatus: new BridgeTriageImportStatus.Importing()));

        Assert.Empty(Fields(pane));
        Assert.Contains("01.flac", Texts(pane));
        Assert.DoesNotContain(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(
                button.Content,
                Loc.Chrome("import.metadata.find_online_ellipsis")));
    }

    [AvaloniaFact]
    public void CompletedImportKeepsSourceContextAndOpensItsAlbum()
    {
        var openedAlbums = new List<string>();
        var (pane, _) = Show(
            Detail(importStatus: new BridgeTriageImportStatus.Complete(
                "release-1", "album-1")),
            openedAlbums: openedAlbums);

        Assert.Empty(Fields(pane));
        Assert.Contains("01.flac", Texts(pane));
        Click(pane, Loc.Chrome("import.view_in_library"));
        Dispatcher.UIThread.RunJobs();
        Assert.Equal(new[] { "album-1" }, openedAlbums);
    }

    // ── Building the pane ────────────────────────────────────────────────────

    private static (ImportMappingPane Pane, AppService App) Show(
        BridgeImportCandidateDetail detail,
        Action<string, BridgeCandidateEditField, string>? onEditField = null,
        BridgeCandidateRuntimeSnapshot? running = null,
        List<string>? identified = null,
        bool identifyAutomatically = true,
        List<BridgeMetadataProvenance>? appliedProvenances = null,
        IReadOnlyList<ReleaseCandidateChoice>? matches = null,
        ImportMetadataPresentation? initialPresentation = null,
        ulong applicationRevision = 1,
        List<Action<BridgeImportCandidateDetail?>>? detailCallbacks = null,
        List<(string Key, BridgeSearchQuery Query)>? searches = null,
        List<string>? openedAlbums = null)
    {
        // Controls may only be built on the headless session's dispatcher
        // thread, which [AvaloniaFact] is what supplies.
        Dispatcher.UIThread.VerifyAccess();

        var import = new ImportService
        {
            ProjectFolderCandidate = NativeBae.ImportCandidateRow,
            // The pane's candidate is seeded below and its own query stays
            // silent, so what the pane reads is exactly the detail handed in.
            SubscribeImportCandidate = (_, onValue, _) =>
            {
                detailCallbacks?.Add(onValue);
                return new NoSubscription();
            },
            // The picked release's library membership is a separate live read;
            // it stays silent so the banner it drives never appears.
            SubscribeReleaseLibraryStatus = (_, _, _, _, _) => new NoSubscription(),
            SetCandidateEditField = (key, field, value) =>
            {
                onEditField?.Invoke(key, field, value);
                return Task.FromResult((true, (string?)null));
            },
            IdentifyFolderForLookup = key =>
            {
                identified?.Add(key);
                return Task.FromResult(true);
            },
            StartCandidateSearch = (key, query) =>
            {
                searches?.Add((key, query));
                return true;
            },
            RetryCandidateSearch = _ => true,
            ClearCandidateSearch = _ => true,
            PreviewFileTags = _ => Task.FromResult((
                true,
                ((BridgeReleaseUserEdit?)FileTagsEdit(), (string?)null))),
            ApplyCandidateExternalMetadata = (_, provenance) =>
            {
                appliedProvenances?.Add(provenance);
                return Task.FromResult((
                    true,
                    ((ulong?)applicationRevision, (string?)null)));
            },
            ApplyCandidateFileTags = _ =>
            {
                appliedProvenances?.Add(new BridgeMetadataProvenance.FileTags());
                return Task.FromResult((
                    true,
                    ((ulong?)applicationRevision, (string?)null)));
            },
            ClearCandidateMetadata = _ => Task.FromResult((
                true,
                ((ulong?)1, (string?)null))),
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
                GetSettings = () => (true, new Settings
                {
                    IdentifyAutomatically = identifyAutomatically,
                    DiscogsUsable = true,
                }),
            });
        app.SettingsStore.Reload();
        var candidate = new ImportCandidate
        {
            Key = CandidateKey,
            Name = "Album",
            FolderPath = CandidateKey,
            Files = detail.Candidate.Files,
            Detail = detail,
        };
        candidate.Matches = matches?.ToList() ?? new List<ReleaseCandidateChoice>();
        candidate.ResolveInitialMetadataPresentation();
        if (initialPresentation is { } presentation)
        {
            candidate.PresentMetadata(presentation);
        }
        app.ImportStore.SeedPreview(
            Array.Empty<BridgeImportListItem>(),
            PreviewData.ImportSummary,
            BridgeTriageTab.Pending,
            new[]
            {
                candidate,
            });
        if (detailCallbacks is not null)
        {
            app.ImportStore.ObserveCandidate(CandidateKey);
        }
        var pane = new ImportMappingPane(
            app,
            new ImportDialogs(
                new ModalHost(),
                new LightboxOverlay(),
                app.Images,
                albumId =>
                {
                    openedAlbums?.Add(albumId);
                    return Task.CompletedTask;
                }));
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
    private static BridgeImportCandidateDetail Detail(
        BridgeImportFailure? failure = null,
        BridgeTriageImportStatus? importStatus = null) =>
        Detail(
            new BridgeMetadataProvenance.ExternalRelease(
                BridgeMetadataSource.MusicBrainz,
                "rel-1",
                []),
            failure: failure,
            importStatus: importStatus);

    private static BridgeImportCandidateDetail Detail(
        BridgeMetadataProvenance? metadataProvenance,
        BridgeImportFailure? failure = null,
        BridgeRawReleaseEdit? edit = null,
        ulong metadataRevision = 1,
        BridgeTriageImportStatus? importStatus = null) =>
        new(
            Candidate: new BridgeFolderCandidate(
                Combination: null,
                CompositionAction: null,
                SourceFileEditsAllowed: true,
                FolderPath: CandidateKey,
                SourceFolderName: "Album",
                WatchedFolderPath: "/Music/Incoming",
                Files: new BridgeCandidateFiles(
                    "mapping-pane-audio",
                    Array.Empty<BridgeCandidateFile>(),
                    new BridgeCandidateSourceAudio(
                        new BridgeSourceAudioSummary.Uniform(
                            new BridgeSourceAudioDescriptor(
                                BridgeSourceAudioLayout.File,
                                SourceAudio)),
                        Array.Empty<BridgeFileInfo>())),
                TrackCount: 2,
                Skipped: false,
                IsAdded: false),
            Actionable: true,
            ResumedIdentifyState: new BridgeIdentifyState.Idle(),
            Row: Row(metadataProvenance, importStatus),
            Release: null,
            PickedLibraryStatus: null,
            FileEvidence: Array.Empty<BridgeFileEvidence>(),
            MetadataDraft: edit
                ?? new BridgeRawReleaseEdit(
                    "Typed Over The Release",
                    ArtistAssignments(),
                    "1991",
                    new BridgeRawPressingEdit(
                        "1996", "CD", "Label Name", "CAT-1", "UK", string.Empty),
                    Array.Empty<BridgeRawTrackEdit>()),
            MetadataDraftIsBlank: edit is not null
                && string.IsNullOrEmpty(edit.AlbumTitle),
            MetadataProvenance: metadataProvenance,
            MetadataRevision: metadataRevision,
            InitialMetadataSource: BridgeDefaultImportMetadataSource.None,
            Mapping: new BridgeMappingTable(
                Array.Empty<BridgeMappingImage>(),
                new[]
                {
                    new BridgeMappingTrackSection(
                        new BridgeTrackSide.Flat(),
                        HeaderKey: null,
                        new BridgeMappingTrackSectionContent.Tracks(new[]
                        {
                            TrackRow("01.flac", "Track One"),
                            TrackRow("02.flac", "Track Two"),
                        })),
                },
                Array.Empty<BridgeMappingFileRow>(),
                Reconciliation: null),
            Cover: new BridgeCoverChoice(
                new BridgeCoverSelection.ReleaseImage("cover.jpg"),
                new BridgeCoverImageSource.Local("/Music/Incoming/Album/cover.jpg"),
                new BridgeCoverImageSource.Local("/Music/Incoming/Album/cover.jpg")),
            Signals: null,
            Failure: failure,
            // This fixture has not visited Find Online or entered a query.
            Session: new BridgeCandidateSession(
                BridgeMetadataPresentation.Draft,
                new BridgeSearchForm(BridgeSearchTab.General, "", "", "", ""),
                null));

    private static BridgeTriageRow Row(
        BridgeMetadataProvenance? metadataProvenance,
        BridgeTriageImportStatus? importStatus) => new(
        CandidateKey: CandidateKey,
        FolderName: "Album",
        WatchedFolderPath: "/Music/Incoming",
        DisplayPath: "Album",
        ResolvedBoundaries: Array.Empty<BridgeResolvedFolderReleaseBoundary>(),
        CombineAncestorKey: null,
        Actionable: true,
        Placement: new BridgeTriagePlacement.Ready(),
        SkipAction: BridgeTriageSkipAction.Skip,
        Actions: [BridgeCandidateAction.ImportReady, BridgeCandidateAction.Identify, BridgeCandidateAction.UseFileMetadata, BridgeCandidateAction.ClearMetadata, BridgeCandidateAction.Skip],
        Matched: null,
        MetadataSummary: null,
        CoverThumbnail: null,
        Selectable: true,
        ImportStatus: importStatus,
        MetadataProvenance: metadataProvenance);

    private static BridgeRawReleaseEdit BlankEdit() => new(
        string.Empty,
        Array.Empty<BridgeArtistAssignment>(),
        string.Empty,
        new BridgeRawPressingEdit(
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty),
        Array.Empty<BridgeRawTrackEdit>());

    private static BridgeTrackMapping TrackRow(string fileId, string title) =>
        new(
            new BridgeMappingSource.File(new BridgeMappingFile(
                FileId: fileId,
                Name: fileId,
                Size: 1024,
                LocalPath: $"/Music/Incoming/Album/{fileId}",
                PreviewTarget: new BridgePreviewTarget(
                    $"/Music/Incoming/Album/{fileId}", 0, null),
                DurationMs: 180_000,
                AudioFormat: SourceAudio,
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
                Position: "1",
                NamedBySource: true),
            DurationMs: 180_000);

    private static BridgeArtistAssignment[] ArtistAssignments() =>
    [
        new BridgeArtistAssignment.New(
            new BridgeNewArtistSeed("Artist Name", null, null, null)),
    ];

    private static BridgeReleaseUserEdit FileTagsEdit() => new(
        "Album Title",
        ArtistAssignments(),
        1991,
        new BridgePressingEdit(1996, "CD", "Label Name", "CAT-1", "UK", null),
        Array.Empty<BridgeTrackUserEdit>());

    private static BridgeReleaseGroup ChoiceGroup(string releaseId)
    {
        var release = new BridgeMetadataResult(
            BridgeMetadataSource.MusicBrainz,
            releaseId,
            1996,
            "CD",
            "Label Name",
            "CAT-1",
            "UK",
            "012345678905",
            "source-group-1");
        return new BridgeReleaseGroup(
                "group-1",
                "Album Title",
                "Artist Name",
                "Label Name",
                null,
                new[]
                {
                    new BridgeReleaseGroupSource(
                        BridgeMetadataSource.MusicBrainz,
                        "https://musicbrainz.org/release-group/source-group-1"),
                },
                1996,
                1996,
                new[]
                {
                    new BridgePressing(
                        new[] { release },
                        new BridgeMetadataProvenance.ExternalRelease(
                            BridgeMetadataSource.MusicBrainz, releaseId, [])),
                });
    }

    private static void Click(Control pane, string label) =>
        Assert.Single(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, label))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static IReadOnlyList<string> Fields(Control pane) =>
        pane.GetLogicalDescendants().OfType<TextBox>()
            .Select(box => box.Text ?? string.Empty).ToList();

    private static TextBox FieldByLabel(Control pane, string label) =>
        Assert.IsType<TextBox>(
            Assert.IsType<StackPanel>(
                Assert.Single(
                    pane.GetLogicalDescendants().OfType<StackPanel>(),
                    panel => panel.Children.OfType<TextBlock>()
                        .Any(text => text.Text == label)))
            .Children.OfType<TextBox>().Single());

    private static IReadOnlyList<string> Texts(Control pane) =>
        pane.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();

    private sealed class NoSubscription : IDisposable
    {
        public void Dispose() { }
    }
}
