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

using static Bae.Desktop.ViewTests.ImportCandidateFixtures;

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
                Loc.Chrome("settings.import.identify_automatically")));
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(
                button.Content,
                Loc.Chrome("import.metadata.search_for_release")));
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

    // Identifying asks for a run whatever the candidate already holds, so
    // pressing it twice is asking twice: the answer a person is unhappy with
    // is exactly the one they press it over.
    [AvaloniaFact]
    public void IdentifyingStartsARunEveryTime()
    {
        var identified = new List<string>();
        var detail = Detail(
            metadataProvenance: null,
            edit: BlankEdit());
        var (pane, _) = Show(detail, identified: identified);

        // Identifying opens the Find online page, where the draft's actions
        // are not; a person comes back to the draft to press it again.
        Click(pane, Loc.Chrome("settings.import.identify_automatically"));
        Click(pane, Loc.Chrome("action.back"));
        Click(pane, Loc.Chrome("settings.import.identify_automatically"));

        Assert.Equal(new[] { CandidateKey, CandidateKey }, identified);
        Assert.Null(detail.MetadataProvenance);
    }

    // Find online is one page, and the second way in asks for nothing: it
    // opens the page on the typed form with no run behind it.
    [AvaloniaFact]
    public void SearchingForAReleaseOpensTheFormAndStartsNoRun()
    {
        var identified = new List<string>();
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            identified: identified);

        Click(pane, Loc.Chrome("import.metadata.search_for_release"));

        Assert.Empty(identified);
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, Loc.Chrome("action.search")));
    }

    [AvaloniaFact]
    public void AutomaticMethodShowsSignalBadgesAndRunAgain()
    {
        var runtime = new BridgeCandidateRuntimeSnapshot(
            new BridgeIdentifyState.NotFoundAnywhere(null),
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

    // The numbers the answers themselves carry stand beside the signal badges,
    // each counted until it is struck out. Striking one out sends back the
    // whole value of what this candidate's identification asks about, with
    // what the run looks up untouched — so nothing is looked up again.
    [AvaloniaFact]
    public void StrikingOutACatalogNumberSendsTheChoiceAndLooksNothingUp()
    {
        var writes = new List<BridgeLookupChoices>();
        var identified = new List<string>();
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()),
            running: RunOffering(new BridgeCatalogAgreement("BST 84055", false)),
            identified: identified,
            initialPresentation: ImportMetadataPresentation.FindOnline,
            lookupChoiceWrites: writes);

        Assert.Contains("BST 84055", Texts(pane));
        Chip(pane, Loc.Chrome("signal.catalog_stop_counting"))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        var written = Assert.Single(writes);
        Assert.Equal(new[] { "BST 84055" }, written.DiscountedCatalogs);
        Assert.Empty(written.ChosenCatalogs);
        Assert.Empty(identified);
    }

    // A number already struck out offers the way back, and taking it sends the
    // value with nothing struck out.
    [AvaloniaFact]
    public void AStruckOutCatalogNumberOffersTheWayBack()
    {
        var writes = new List<BridgeLookupChoices>();
        var detail = Detail(
            metadataProvenance: null,
            edit: BlankEdit(),
            lookupChoices: new BridgeLookupChoices(
                DiscIdExcluded: false,
                BarcodeExcluded: false,
                ChosenCatalogs: [],
                DiscountedCatalogs: ["BST 84055"]));
        var (pane, _) = Show(
            detail,
            running: RunOffering(new BridgeCatalogAgreement("BST 84055", true)),
            initialPresentation: ImportMetadataPresentation.FindOnline,
            lookupChoiceWrites: writes);

        Chip(pane, Loc.Chrome("signal.catalog_count_again"))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Empty(Assert.Single(writes).DiscountedCatalogs);
    }

    // Striking a number out is not the same decision as taking one out of the
    // run: the numbers the run looks up are carried through untouched.
    [AvaloniaFact]
    public void StrikingANumberOutLeavesWhatTheRunLooksUpAlone()
    {
        var writes = new List<BridgeLookupChoices>();
        var detail = Detail(
            metadataProvenance: null,
            edit: BlankEdit(),
            lookupChoices: new BridgeLookupChoices(
                DiscIdExcluded: true,
                BarcodeExcluded: false,
                ChosenCatalogs: ["BST 84055"],
                DiscountedCatalogs: []));
        var (pane, _) = Show(
            detail,
            running: RunOffering(new BridgeCatalogAgreement("BST 84055", false)),
            initialPresentation: ImportMetadataPresentation.FindOnline,
            lookupChoiceWrites: writes);

        Chip(pane, Loc.Chrome("signal.catalog_stop_counting"))
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        var written = Assert.Single(writes);
        Assert.True(written.DiscIdExcluded);
        Assert.Equal(new[] { "BST 84055" }, written.ChosenCatalogs);
        Assert.Equal(new[] { "BST 84055" }, written.DiscountedCatalogs);
    }

    [AvaloniaFact]
    public void TheSearchFormStartsBlankAndKeepsWhatIsTypedAcrossQueryTypes()
    {
        var (pane, _) = Show(
            Detail(metadataProvenance: null, edit: BlankEdit()));

        Click(pane, Loc.Chrome("import.metadata.search_for_release"));
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
            Detail(metadataProvenance: null, edit: BlankEdit()));

        Click(pane, Loc.Chrome("import.metadata.search_for_release"));

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
            searches: searches);

        Click(pane, Loc.Chrome("import.metadata.search_for_release"));
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
            new BridgeSourceSearchEntry[]
            {
                new(
                    BridgeMetadataSource.MusicBrainz,
                    new BridgeSourceSearch.Failed(new BridgeLookupFailure.Network())),
                new(
                    BridgeMetadataSource.Discogs,
                    new BridgeSourceSearch.NotConfigured()),
            },
            Array.Empty<BridgeReleaseGroup>(),
            new Dictionary<string, BridgeLibraryStatus>(),
            BridgeSearchStatus.Failed);
        var runtime = new BridgeCandidateRuntimeSnapshot(
            new BridgeIdentifyState.NotFoundAnywhere(null),
            new BridgeSignalsToolbar(Array.Empty<BridgeToolbarSignal>()),
            null, search);
        var (pane, _) = Show(Detail(), running: runtime,
            initialPresentation: ImportMetadataPresentation.FindOnline);

        Assert.DoesNotContain(Loc.Chrome("search.no_matches"), Texts(pane));
        Assert.Contains(Loc.Chrome("import.search.source_not_configured", "source", "Discogs"), Texts(pane));
    }

    // Picking a pressing spins on the row being read and closes the browser
    // when that read lands. The pane waits on the read alone: the candidate is
    // re-read whenever anything about it moves, and none of that is the
    // pick's answer.
    [AvaloniaFact]
    public void PickingAReleaseSpinsOnItsRowUntilTheReadLands()
    {
        var detailCallbacks = new List<Action<BridgeImportCandidateDetail?>>();
        var gate = new TaskCompletionSource();
        var provenance = new BridgeMetadataProvenance.ExternalRelease(
            BridgeMetadataSource.MusicBrainz,
            "rel-1",
            []);
        var (pane, _) = Show(
            Detail(provenance, metadataRevision: 1),
            running: RunOfferingChoice("rel-1"),
            initialPresentation: ImportMetadataPresentation.FindOnline,
            applicationGate: gate,
            detailCallbacks: detailCallbacks);

        var choices = Assert.Single(
            pane.GetLogicalDescendants().OfType<ListBox>());
        choices.SelectedIndex = 0;
        Dispatcher.UIThread.RunJobs();

        // The row being read says so, and cannot be picked a second time.
        Assert.Single(
            pane.GetLogicalDescendants().OfType<Spinner>(),
            spinner => spinner.IsVisible);
        Assert.False(
            Assert.Single(pane.GetLogicalDescendants().OfType<ListBox>()).IsEnabled);

        // A re-read of the candidate is not the read the pick is waiting on.
        detailCallbacks[0](Detail(provenance, metadataRevision: 2));
        Dispatcher.UIThread.RunJobs();
        Assert.Contains(
            pane.GetLogicalDescendants().OfType<ListBox>(),
            list => list.Items.Count == 1);

        gate.SetResult();
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
                Loc.Chrome("settings.import.identify_automatically")));
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
        List<BridgeMetadataProvenance>? appliedProvenances = null,
        IReadOnlyList<ReleaseCandidateChoice>? matches = null,
        ImportMetadataPresentation? initialPresentation = null,
        ulong applicationRevision = 1,
        TaskCompletionSource? applicationGate = null,
        List<Action<BridgeImportCandidateDetail?>>? detailCallbacks = null,
        List<(string Key, BridgeSearchQuery Query)>? searches = null,
        List<string>? openedAlbums = null,
        List<BridgeLookupChoices>? lookupChoiceWrites = null)
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
            RerunIdentifyForCandidate = key =>
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
            // A gate, where one is handed in, holds the read open so a test
            // can act while the pick is still in flight.
            ApplyCandidateExternalMetadata = async (_, provenance) =>
            {
                appliedProvenances?.Add(provenance);
                if (applicationGate is not null)
                {
                    await applicationGate.Task;
                }
                return (true, ((ulong?)applicationRevision, (string?)null));
            },
            ApplyCandidateFileTags = async _ =>
            {
                appliedProvenances?.Add(new BridgeMetadataProvenance.FileTags());
                if (applicationGate is not null)
                {
                    await applicationGate.Task;
                }
                return (true, ((ulong?)applicationRevision, (string?)null));
            },
            ClearCandidateMetadata = _ => Task.FromResult((
                true,
                ((ulong?)1, (string?)null))),
            // The run the pane and the progress line both read. Absent leaves
            // the candidate at rest, where the card offers the Import button.
            CandidateRuntime = _ => running,
            // What a badge or a chip sends back. Core decides whether it also
            // starts a run; nothing here does.
            SetCandidateLookupChoices = (_, choices) =>
            {
                lookupChoiceWrites?.Add(choices);
                return Task.FromResult((true, (string?)null));
            },
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

    /// <summary>A verdict whose signals narrowed nothing out.</summary>
    /// <summary>A settled run offering `agreements` as its catalog chips, with
    /// one barcode signal so the badge row has a badge beside them.</summary>
    private static BridgeCandidateRuntimeSnapshot RunOffering(
        params BridgeCatalogAgreement[] agreements) =>
        new(
            new BridgeIdentifyState.Found(
                null,
                Array.Empty<BridgeReleaseGroup>(),
                new Dictionary<string, BridgeLibraryStatus>(),
                1,
                new Dictionary<string, BridgeAgreements>(),
                NothingNarrowedOut(),
                agreements),
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

    /// <summary>The one chip whose tooltip is `tip`.</summary>
    private static Button Chip(Control pane, string tip) =>
        Assert.Single(
            pane.GetLogicalDescendants().OfType<Button>(),
            button => Equals(ToolTip.GetTip(button), tip));

    private static BridgeNarrowedOut NothingNarrowedOut() =>
        new(
            Array.Empty<BridgeReleaseGroup>(),
            new Dictionary<string, BridgeLibraryStatus>(),
            new Dictionary<string, BridgeAgreements>());

    /// <summary>A settled run whose one group offers `releaseId` to pick.</summary>
    private static BridgeCandidateRuntimeSnapshot RunOfferingChoice(string releaseId) =>
        new(
            new BridgeIdentifyState.Found(
                null,
                new[] { ChoiceGroup(releaseId) },
                new Dictionary<string, BridgeLibraryStatus>(),
                1,
                new Dictionary<string, BridgeAgreements>(),
                NothingNarrowedOut(),
                []),
            new BridgeSignalsToolbar(Array.Empty<BridgeToolbarSignal>()),
            null,
            null);

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
