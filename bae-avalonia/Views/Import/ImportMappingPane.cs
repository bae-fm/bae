using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Threading;
using uniffi.bae_bridge;
namespace Bae.Desktop;

/// <summary>
/// The one surface a folder becomes a release on, to the right of the triage
/// sidebar: two sections and a commit bar. Section one is what the folder is
/// being read as; section two is every source unit it offers alongside the track
/// committing makes of it.
///
/// There is no identify⇄confirm layout flip. The table is the same table before
/// and after a release is picked — picking one fills its BECOMES column in
/// place, and the metadata slot switches between the draft, Find online, and
/// File tags without emptying it. Nothing in the commit bar ever disables the
/// commit: a disagreement is stated, and the one refusal left in the whole
/// import is audio that will not decode, which core raises.
///
/// State lives here for one selected candidate. The pane rebuilds on a
/// structural change — a different candidate, a different release, a sheet
/// binding, a role — and never on a store tick, so a field the user is typing in
/// keeps its focus and its caret. It does read the queue on a tick, for the one
/// case where a tick IS a structural change: the verdict for the folder it is
/// showing settling on a release nobody has picked yet.
/// </summary>
internal sealed partial class ImportMappingPane : UserControl
{
    private readonly AppService _app;
    private readonly ImportStore _import;
    private readonly ImportDialogs _dialogs;

    private readonly ContentControl _content = new()
    {
        HorizontalContentAlignment = HorizontalAlignment.Stretch,
        VerticalContentAlignment = VerticalAlignment.Stretch,
    };

    // The candidate under the pane, what the pane last read about it, and
    // what is running for it right now — the run whose state the pane shows,
    // and how far a running import has got. The last of those is not stored
    // anywhere else: the pane subscribes to the candidate-runtime signal and
    // filters it to its own key.
    private string? _key;
    private ImportCandidate? _candidate;
    private string _candidatePresentationFingerprint = string.Empty;
    private BridgeCandidateRuntimeSnapshot? _runtime;

    // Whether a pick is in flight. The control that started it says so; the
    // pane behind it keeps showing whatever is stored until the pick lands.
    private bool _pickInFlight;
    private BridgeMetadataProvenance? _applyingProvenance;
    private ulong? _applicationCommandRevision;
    private ulong? _applicationDetailRevision;

    // The search editor, held open across rebuilds along with what has been
    // typed into it: the pane re-renders whenever a result lands, and a query
    // that reset itself each time would be unusable.
    // Which half of the search editor is showing: what identification found,
    // or the typed search. The editor owns it, so opening it again starts on
    // the matches rather than wherever the last session left off.
    private bool _searchManual;
    private List<ReleaseCandidateChoice> _searchResults = new();
    private string _searchArtist = string.Empty;
    private string _searchAlbum = string.Empty;
    private int _searchSource;
    private string _searchError = string.Empty;

    private ImportMappingTable? _table;

    internal ImportMappingPane(AppService app, ImportDialogs dialogs)
    {
        _app = app;
        _import = app.ImportStore;
        _dialogs = dialogs;

        Content = _content;
        _import.CandidateRuntimeChanged += OnCandidateRuntimeChanged;
        _import.PreviewElapsedChanged += OnPreviewChanged;
        _import.Changed += OnQueueChanged;
        _import.ReleaseLibraryStatusChanged += Render;
        Render();
    }

    /// <summary>Put a candidate under the pane. Everything it draws — the
    /// picked release, the metadata form, the table, the cover — is already
    /// stored under the candidate, so showing it is one read and no
    /// spinner.</summary>
    internal Task ShowCandidate(BridgeTriageRow row)
    {
        StopPreview();
        _key = row.CandidateKey;
        _candidate = _import.Candidate(row.CandidateKey);
        _candidatePresentationFingerprint = CandidatePresentationFingerprint(_candidate);
        // The subscription is already open, so this read cannot be undone by a
        // change that was on its way.
        _runtime = _import.CandidateRuntime(row.CandidateKey);
        ResetSession();
        ObserveRelease();
        Render();
        return Task.CompletedTask;
    }

    // Every change to this candidate arrives as a queue tick: a write the pane
    // made, a run that settled, an import that failed. Re-render only when the
    // value actually moved — a re-render rebuilds the editor's controls, and
    // doing that on every tick would drop focus mid-typing.
    private void OnQueueChanged()
    {
        if (_key is not { } key)
        {
            return;
        }
        var refreshed = _import.Candidate(key);
        var presentationFingerprint = CandidatePresentationFingerprint(refreshed);
        if (Equals(refreshed?.Detail, _candidate?.Detail)
            && IdentifyFingerprint(refreshed) == IdentifyFingerprint(_candidate)
            && presentationFingerprint == _candidatePresentationFingerprint)
        {
            return;
        }
        var provenanceChanged = !Equals(
            refreshed?.MetadataProvenance,
            _candidate?.MetadataProvenance);
        _candidate = refreshed;
        _candidatePresentationFingerprint = presentationFingerprint;
        if (_applyingProvenance is { } applying
            && Equals(refreshed?.MetadataProvenance, applying))
        {
            _applicationDetailRevision = refreshed?.Detail?.MetadataRevision;
            FinishMetadataApplicationIfConfirmed();
        }
        if (provenanceChanged)
        {
            ObserveRelease();
        }
        Render();
    }

    // What one key has in flight changed. A progress tick within a running
    // import moves nothing the pane draws, so it re-renders only when the run
    // itself has moved.
    private void OnCandidateRuntimeChanged(BridgeCandidateRuntimeChange change)
    {
        if (_key is not { } key)
        {
            return;
        }
        var next = change switch
        {
            BridgeCandidateRuntimeChange.Updated updated when updated.Key == key =>
                updated.Runtime,
            BridgeCandidateRuntimeChange.Removed removed when removed.Key == key =>
                null,
            BridgeCandidateRuntimeChange.Reset reset => reset.Runtimes
                .FirstOrDefault(entry => entry.Key == key)?.Runtime,
            _ => _runtime,
        };
        if (RuntimeFingerprint(next) == RuntimeFingerprint(_runtime))
        {
            _runtime = next;
            return;
        }
        _runtime = next;
        Render();
    }

    /// <summary>The run in flight for the open key, else what the candidate's
    /// tables say.</summary>
    private ImportCandidateRowStatus EffectiveRowStatus =>
        _import.ProjectRun(_runtime).RowStatus
            ?? _candidate?.RowStatus
            ?? new ImportCandidateRowStatus();

    /// <summary>The pressings the open key is being offered: the run in
    /// flight's while it has any, else the stored verdict's.</summary>
    private List<ReleaseCandidateChoice> EffectiveMatches =>
        _import.ProjectRun(_runtime).Matches is { Count: > 0 } live
            ? live
            : _candidate?.Matches ?? new List<ReleaseCandidateChoice>();

    private string RuntimeFingerprint(BridgeCandidateRuntimeSnapshot? runtime)
    {
        var (status, matches, signals) = _import.ProjectRun(runtime);
        return (status?.Kind ?? string.Empty)
            + "|"
            + string.Join(",", matches.Select(choice => choice.ReleaseId))
            + "|"
            + string.Join(
                ",",
                signals.Select(badge => $"{badge.Kind}:{badge.State.Kind}:{badge.Excluded}"));
    }

    private static string IdentifyFingerprint(ImportCandidate? candidate) =>
        candidate is null
            ? string.Empty
            : candidate.RowStatus.Kind + "|"
                + string.Join(",", candidate.Matches.Select(choice => choice.ReleaseId));

    private static string CandidatePresentationFingerprint(ImportCandidate? candidate) =>
        candidate is null
            ? string.Empty
            : $"{candidate.MetadataPresentation}|{candidate.FileTagsPreviewStatus}"
                + $"|{candidate.FileTagsPreviewError}";

    // Watch the picked release's library membership, so the banner above the
    // table says when this folder is already in the library.
    private void ObserveRelease()
    {
        _import.ClearReleaseLibraryStatus();
        if (_candidate?.PickedRelease is not { } picked)
        {
            return;
        }
        _import.ObserveReleaseLibraryStatus(
            picked.Source,
            picked.ReleaseId,
            _candidate.Release?.SourceGroupId);
    }

    /// <summary>Raised when the pane empties itself — a committed folder is no
    /// longer a candidate to map, so the row that put it here stops reading as
    /// the selected one.</summary>
    internal event Action? Cleared;

    /// <summary>Empty the pane — no candidate is selected.</summary>
    internal void Clear()
    {
        StopPreview();
        _key = null;
        _candidate = null;
        _candidatePresentationFingerprint = string.Empty;
        ResetSession();
        Render();
        Cleared?.Invoke();
    }

    // The audition belongs to the folder under the pane; when that folder
    // leaves, so does it.
    private void StopPreview()
    {
        _app.Playback.PreviewStop();
        _import.ClearPreview();
    }

    // The only state the pane owns: the search editor's own form, the storage
    // choice, and the last command's failure. Everything else is stored.
    private void ResetSession()
    {
        _import.ClearReleaseLibraryStatus();
        _pickInFlight = false;
        _applyingProvenance = null;
        _applicationCommandRevision = null;
        _applicationDetailRevision = null;
        _searchManual = false;
        _searchResults = new List<ReleaseCandidateChoice>();
        _searchArtist = string.Empty;
        _searchAlbum = _candidate?.Name ?? string.Empty;
        _searchSource = 0;
        _searchError = string.Empty;
        _commitError = string.Empty;
    }

    // ── Selecting and presenting metadata sources ──────────────────────────

    private async Task ApplyMetadata(BridgeMetadataProvenance provenance)
    {
        if (_key is not { } key)
        {
            return;
        }
        _applyingProvenance = provenance;
        _applicationCommandRevision = null;
        _applicationDetailRevision = null;
        _pickInFlight = true;
        Render();
        try
        {
            var revision = provenance switch
            {
                BridgeMetadataProvenance.ExternalRelease external =>
                    await _import.ApplyCandidateExternalMetadata(
                        key, external.Source, external.ReleaseId),
                BridgeMetadataProvenance.FileTags =>
                    await _import.ApplyCandidateFileTags(key),
                _ => throw new ArgumentOutOfRangeException(
                    nameof(provenance), provenance, "Unknown metadata provenance"),
            };
            if (revision is null)
            {
                _applyingProvenance = null;
                return;
            }
            _applicationCommandRevision = revision;
            FinishMetadataApplicationIfConfirmed();
        }
        finally
        {
            _pickInFlight = false;
            Render();
        }
    }

    private void FinishMetadataApplicationIfConfirmed()
    {
        if (_applicationCommandRevision is null
            || _applicationCommandRevision != _applicationDetailRevision
            || _key is not { } key)
        {
            return;
        }
        _applyingProvenance = null;
        _applicationCommandRevision = null;
        _applicationDetailRevision = null;
        _import.PresentMetadata(key, ImportMetadataPresentation.Draft);
    }

    /// <summary>Present one source without selecting it. Only this explicit
    /// segment action writes mode history or starts source-specific work.</summary>
    private void PresentMetadata(ImportMetadataPresentation presentation)
    {
        if (_key is not { } key || _candidate is not { } candidate)
        {
            return;
        }
        _import.PresentMetadata(key, presentation);
        switch (presentation)
        {
            case ImportMetadataPresentation.Draft:
                break;
            case ImportMetadataPresentation.FindOnline:
                _searchManual = !(_app.SettingsStore.Current?
                    .AutomaticImportIdentification ?? false);
                if (!_searchManual && ShownIdentifyState is BridgeIdentifyState.Idle)
                {
                    _ = _import.StartInteractiveLookup(key);
                }
                break;
            case ImportMetadataPresentation.FileTags:
                _ = _import.LoadFileTagsPreview(key);
                break;
            default:
                throw new ArgumentOutOfRangeException(
                    nameof(presentation), presentation, "Unknown metadata presentation");
        }
        Render();
    }

    private BridgeIdentifyState ShownIdentifyState =>
        _runtime?.IdentifyState
            ?? _candidate?.Detail?.ResumedIdentifyState
            ?? new BridgeIdentifyState.Idle();

    private void OnPreviewChanged() => _table?.ApplyPreviewAccent();

    // ── Render ───────────────────────────────────────────────────────────────

    private void Render()
    {
        if (_key is null)
        {
            _content.Content = EmptyState();
            return;
        }
        if (_candidate is null)
        {
            // A folder is chosen and its read has not landed yet. Blank, not
            // the placeholder: "pick a folder" would be false about a folder
            // that is picked, and it was flashing on every click.
            _content.Content = PendingState();
            return;
        }

        var sections = new StackPanel { Spacing = 18, Margin = new Thickness(20, 16, 20, 16) };
        sections.Children.Add(FolderLine());
        sections.Children.Add(BuildMetadataSource().Build());
        if (_import.ReleaseLibraryStatus is { } status)
        {
            sections.Children.Add(BuildLibraryStatusBanner(status));
        }
        if (_candidate?.Failure is { } failure && EffectiveRowStatus.Kind.Length == 0)
        {
            sections.Children.Add(BuildFailureBanner(failure));
        }
        _table = null;
        if (_candidate is { Detail: not null })
        {
            var mapping = _candidate.Mapping;
            var actions = MappingActions();
            if (mapping.Images.Length > 0)
            {
                sections.Children.Add(new ImportMappingGallery(
                    mapping.Images,
                    _candidate.FileEvidence,
                    (image, path) => _app.Images.Bind(
                        image, new ImageContent.LocalFile(path), ImageWidths.PickerTile),
                    actions.OpenImages).Build());
            }
            _table = new ImportMappingTable(
                mapping,
                sheetFileId => _import.SheetBindingOptions(_key!, sheetFileId),
                () => _import.PreviewingPath,
                _app.Library,
                actions,
                _candidate.Detail!.Unprobed,
                _candidate.FileEvidence);
            // Both sections and their headings, as one block.
            sections.Children.Add(_table.Build());
        }

        _content.Content = new ScrollViewer { Content = sections };
    }

    /// <summary>
    /// How long a candidate's read may take before it is worth saying anything
    /// about. Under this the pane stays blank — the read almost always lands
    /// first, and a spinner that appears and leaves that fast is a flash of its
    /// own.
    /// </summary>
    private static readonly TimeSpan SpinnerDelay = TimeSpan.FromMilliseconds(150);

    /// <summary>The pane for a selection whose candidate has not been delivered
    /// yet: empty, and a spinner only if the wait outlasts
    /// <see cref="SpinnerDelay"/>. The timer stops when the pane is replaced by
    /// the delivered candidate, which is what detaches this.</summary>
    private static Control PendingState()
    {
        var host = new ContentControl
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var timer = new DispatcherTimer { Interval = SpinnerDelay };
        timer.Tick += (_, _) =>
        {
            timer.Stop();
            host.Content = new Spinner { Width = 16, Height = 16 };
        };
        host.AttachedToVisualTree += (_, _) => timer.Start();
        host.DetachedFromVisualTree += (_, _) => timer.Stop();
        return host;
    }

    private static Control EmptyState()
    {
        var text = new TextBlock
        {
            Text = Loc.Chrome("import.pane.pick_a_candidate"),
            FontSize = 13,
            TextWrapping = TextWrapping.Wrap,
            MaxWidth = 320,
            TextAlignment = TextAlignment.Center,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return text;
    }

    // The folder the pane is about, at the top of it: what it is called on disk
    // and what audio it holds. It leads the pane because it is the one fact
    // nothing below can change — the release, the metadata and the mapping are
    // all readings of this folder. The name is selectable (a path is something
    // people copy) and the glyph beside it shows the folder in the file
    // manager.
    private Control FolderLine()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var reveal = Icons.IconButton(Icons.Folder, 14, "BaeTextSecondaryBrush", 22);
        var path = _key;
        ToolTip.SetTip(reveal, Loc.Chrome("libraries.reveal"));
        reveal.Click += (_, _) =>
        {
            if (path is not null)
            {
                RevealInFileManager.Reveal(path);
            }
        };
        row.Children.Add(reveal);

        var name = new SelectableTextBlock
        {
            Text = _candidate?.Name ?? string.Empty,
            FontSize = 15,
            FontFamily = new FontFamily("monospace"),
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        name[!SelectableTextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        row.Children.Add(name);

        if (_candidate?.Files?.FormatLabel is { Length: > 0 } formatLabel)
        {
            var format = new TextBlock
            {
                Text = formatLabel,
                FontSize = 12,
                VerticalAlignment = VerticalAlignment.Center,
            };
            format[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            row.Children.Add(format);
        }
        return row;
    }

    // ── Section 1: metadata ──────────────────────────────────────────────────

    private ImportMetadataSourceSection BuildMetadataSource() => new()
    {
        Presentation = _candidate?.MetadataPresentation
            ?? ImportMetadataPresentation.Draft,
        DraftIsBlank = DraftIsBlank(),
        CommitRow = DraftIsBlank() ? null : BuildCommitRow(),
        Title = MetadataTitle(),
        Edit = _candidate?.Edit,
        MetaLine = MetaLine(),
        ProvenanceLabel = ProvenanceLabel(),
        ProvenanceUri = ProvenanceUri(),
        IsReading = _pickInFlight
            || _applyingProvenance is not null
            || _candidate?.FileTagsPreviewStatus == ImportFileTagsPreviewStatus.Loading,
        FileTagsPreview = _candidate?.FileTagsPreview,
        FileTagsMetaLine = FileTagsMetaLine(),
        FileTagsError = _candidate?.FileTagsPreviewError,
        LookupOptions = _candidate?.MetadataPresentation
            == ImportMetadataPresentation.FindOnline
            ? BuildSearchEditor()
            : null,
        LoadCover = _candidate?.Cover is { } cover
            ? image => _app.Images.Bind(
                image, ImportDialogs.CoverChoiceContent(cover), ImageWidths.PickerTile)
            : null,
        HasCoverOptions = _candidate?.LocalArtwork.Count > 0
            || (_candidate?.Release?.CoverArt.Length ?? 0) > 0,
        Library = _app.Library,
        OnPresent = PresentMetadata,
        OnReadFileTags = () =>
        {
            if (_key is { } key)
            {
                _ = _import.LoadFileTagsPreview(key);
            }
        },
        OnUseFileTags = () => _ = ApplyMetadata(
            new BridgeMetadataProvenance.FileTags()),
        OnClearMetadata = () => _ = ClearMetadata(),
        OnEditCover = () => _ = ChooseCover(),
        OnSelectCover = selection => _ = SetCover(selection),
        OnEditField = (field, value) => _ = SetEditField(field, value),
        OnEditArtists = assignments => _ = SetAlbumArtists(assignments),
    };

    private async Task ClearMetadata()
    {
        if (_key is not { } key)
        {
            return;
        }
        await _dialogs.ConfirmClearMetadata(async () =>
        {
            await _import.ClearCandidateMetadata(key);
        });
    }

    private async Task SetEditField(BridgeCandidateEditField field, string value)
    {
        if (_key is { } key)
        {
            await _import.SetCandidateEditField(key, field, value);
        }
    }

    private async Task SetAlbumArtists(IReadOnlyList<BridgeArtistAssignment> assignments)
    {
        if (_key is { } key)
        {
            await _import.SetCandidateAlbumArtists(key, assignments);
        }
    }

    private async Task SetCover(BridgeCoverSelection selection)
    {
        if (_key is { } key)
        {
            await _import.SetCandidateCover(key, selection);
        }
    }

    /// <summary>"CD · 1996 · XE · 463360 2 · 10 tracks", from the live edit and
    /// the live table so it tracks what is being committed. Empty pressing
    /// fields drop out rather than leaving stray separators.</summary>
    private string MetaLine()
    {
        if (_candidate?.Edit is not { } edit)
        {
            return _candidate?.Files?.FormatLabel ?? string.Empty;
        }
        var mapping = _candidate.Mapping;
        string[] lead =
        [
            edit.Pressing.Format,
            edit.Pressing.Year,
            edit.Pressing.Country,
            edit.Pressing.CatalogNumber,
        ];
        return string.Join(
            "  ·  ",
            lead
                .Append(Loc.Chrome("import.candidate.tracks", "count", mapping.WillWriteCount()))
                .Where(part => part.Length > 0));
    }

    private string? ProvenanceLabel() => _candidate?.MetadataProvenance switch
    {
        BridgeMetadataProvenance.ExternalRelease external =>
            BaeBridgeMethods.BridgeMetadataSourceName(external.Source),
        BridgeMetadataProvenance.FileTags =>
            Loc.Core("ui.import.metadata.file_tags"),
        null => null,
        _ => throw new ArgumentOutOfRangeException(
            nameof(_candidate.MetadataProvenance),
            _candidate.MetadataProvenance,
            "Unknown metadata provenance"),
    };

    private Uri? ProvenanceUri() => _candidate?.MetadataProvenance switch
    {
        BridgeMetadataProvenance.ExternalRelease external =>
            ExternalReleaseUri(external),
        BridgeMetadataProvenance.FileTags or null => null,
        _ => throw new ArgumentOutOfRangeException(
            nameof(_candidate.MetadataProvenance),
            _candidate.MetadataProvenance,
            "Unknown metadata provenance"),
    };

    private static Uri ExternalReleaseUri(
        BridgeMetadataProvenance.ExternalRelease external)
    {
        var root = external.Source switch
        {
            BridgeMetadataSource.MusicBrainz =>
                "https://musicbrainz.org/release/",
            BridgeMetadataSource.Discogs =>
                "https://www.discogs.com/release/",
            _ => throw new ArgumentOutOfRangeException(
                nameof(external.Source),
                external.Source,
                "Unknown metadata source"),
        };
        return new Uri(root + Uri.EscapeDataString(external.ReleaseId));
    }

    private bool DraftIsBlank() => _candidate?.Detail?.MetadataDraftIsBlank ?? true;

    private string FileTagsMetaLine()
    {
        if (_candidate?.FileTagsPreview is not { } preview)
        {
            return _candidate?.Files?.FormatLabel ?? string.Empty;
        }
        return string.Join(
            "  ·  ",
            Loc.Core("ui.import.metadata.from_file_tags"),
            Loc.Chrome("import.candidate.tracks", "count", preview.Tracks.LongLength));
    }

    private string MetadataTitle()
    {
        if (_candidate?.Edit?.AlbumTitle is { Length: > 0 } title)
        {
            return title;
        }
        return Loc.Chrome("import.metadata.album_title_placeholder");
    }

    private async Task ChooseCover()
    {
        if (_key is not { } key || _candidate is not { } candidate)
        {
            return;
        }
        await _dialogs.ShowCoverPicker(
            candidate.Release?.CoverArt.ToList() ?? new List<BridgeRemoteCover>(),
            candidate.LocalArtwork,
            async picked =>
            {
                if (picked is { } choice)
                {
                    await _import.SetCandidateCover(key, choice.Selection);
                }
            });
        Render();
    }

    // Search is the metadata section's editor, opened from its change control —
    // not a pane mounted alongside it. It has two halves and shows one at a
    // time: what identification already found for this folder (the common
    // case, one click), and the typed search for when it found nothing.
    private Control BuildSearchEditor()
    {
        var column = new StackPanel { Spacing = 8 };
        column.Children.Add(ImportPaneUi.ZoneTitle(Loc.Core("ui.import.header.find_release")));
        column.Children.Add(_searchManual ? ManualHalf() : SignalsHalf());
        if (_searchError.Length > 0)
        {
            column.Children.Add(ImportPaneUi.Cell(_searchError, secondary: true));
        }
        return column;
    }

    // What identification made of the folder: its matches, or the one line
    // saying it has none — with the way over to the typed search beside it.
    private Control SignalsHalf()
    {
        var column = new StackPanel { Spacing = 8 };
        var matches = EffectiveMatches;
        if (matches.Count > 0)
        {
            column.Children.Add(ChoiceList(matches));
        }
        else
        {
            column.Children.Add(ImportPaneUi.Cell(
                Loc.Chrome("import.search.no_automatic_matches"),
                secondary: true));
        }

        var manual = ImportPaneUi.RowButton(Loc.Chrome("import.row.search_manually"));
        manual.Click += (_, _) => { _searchManual = true; Render(); };
        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(manual);
        column.Children.Add(actions);
        return column;
    }

    // The typed search. Signals goes back to what identification found;
    // Identify automatically reruns extraction and lookup.
    private Control ManualHalf()
    {
        var column = new StackPanel { Spacing = 8 };

        var back = ImportPaneUi.RowButton(Loc.Chrome("import.search.signals"));
        back.Click += (_, _) => { _searchManual = false; Render(); };
        var auto = ImportPaneUi.RowButton(
            Loc.Chrome("import.search.identify_automatically"));
        auto.Click += (_, _) =>
        {
            _searchManual = false;
            if (_key is { } key)
            {
                _ = _import.StartInteractiveLookup(key);
            }
            Render();
        };
        var header = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        header.Children.Add(back);
        header.Children.Add(auto);
        column.Children.Add(header);

        var artistField = DialogUi.Field(Loc.Chrome("import.field.artist_manual"), out var artistBox);
        artistBox.Text = _searchArtist;
        artistBox.TextChanged += (_, _) => _searchArtist = artistBox.Text ?? string.Empty;
        var albumField = DialogUi.Field(Loc.Chrome("search.field.album"), out var albumBox);
        albumBox.Text = _searchAlbum;
        albumBox.TextChanged += (_, _) => _searchAlbum = albumBox.Text ?? string.Empty;
        var sourceBox = new ComboBox
        {
            ItemsSource = new[] { "discogs", "musicbrainz" },
            SelectedIndex = _searchSource,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        sourceBox.SelectionChanged += (_, _) => _searchSource = sourceBox.SelectedIndex;
        var sourceField = new StackPanel { Spacing = 4 };
        sourceField.Children.Add(DialogUi.SectionLabel(Loc.Chrome("search.field.source")));
        sourceField.Children.Add(sourceBox);

        var fields = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*,Auto"), ColumnSpacing = 8 };
        Grid.SetColumn(artistField, 0);
        Grid.SetColumn(albumField, 1);
        Grid.SetColumn(sourceField, 2);
        fields.Children.Add(artistField);
        fields.Children.Add(albumField);
        fields.Children.Add(sourceField);
        column.Children.Add(fields);

        var search = ImportPaneUi.RowButton(Loc.Chrome("action.search"));
        search.Click += async (_, _) =>
        {
            search.IsEnabled = false;
            var (current, found) = await _app.Import.SearchReleases(
                (string)sourceBox.SelectedItem!, _searchArtist, _searchAlbum);
            search.IsEnabled = true;
            if (!current)
            {
                return;
            }
            if (found.Error is not null)
            {
                _searchError = found.Error;
                Render();
                return;
            }
            _searchResults = found.Candidates ?? new List<ReleaseCandidateChoice>();
            _searchError = _searchResults.Count == 0 ? Loc.Chrome("search.no_matches") : string.Empty;
            Render();
        };

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(search);
        column.Children.Add(actions);

        if (_searchResults.Count > 0)
        {
            column.Children.Add(ChoiceList(_searchResults));
        }
        return column;
    }

    // The pressings on offer, whichever half produced them. Picking one
    // applies that external release, and the editor closes only after the
    // candidate subscription delivers that provenance too.
    private Control ChoiceList(List<ReleaseCandidateChoice> choices)
    {
        var results = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 190 };
        results.ItemsSource = choices.Select(ChoiceRow).ToList();
        results.IsEnabled = _applyingProvenance is null;
        results.SelectionChanged += async (_, _) =>
        {
            if (results.SelectedIndex < 0 || results.SelectedIndex >= choices.Count)
            {
                return;
            }
            var chosen = choices[results.SelectedIndex];
            await ApplyMetadata(
                new BridgeMetadataProvenance.ExternalRelease(
                    chosen.Source,
                    chosen.ReleaseId));
        };
        return results;
    }

    private Control ChoiceRow(ReleaseCandidateChoice choice)
    {
        var row = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            ColumnSpacing = 8,
            Margin = new Thickness(4, 6),
        };
        row.Children.Add(ImportPaneUi.Cell(choice.Summary));
        var progress = new Spinner
        {
            Width = 14,
            Height = 14,
            IsVisible = _applyingProvenance is BridgeMetadataProvenance.ExternalRelease applying
                && applying.Source == choice.Source
                && applying.ReleaseId == choice.ReleaseId,
        };
        Grid.SetColumn(progress, 1);
        row.Children.Add(progress);
        return row;
    }

    // ── Section 2: what the table's controls do ──────────────────────────────

    private ImportMappingActions MappingActions() => new(
        SetRole: (fileId, choice) => _ = SetRole(fileId, choice),
        BindSheet: (sheetFileId, audioFileId) => _ = SetSheetBinding(sheetFileId, audioFileId),
        SetSheetDisc: (sheetFileId, disc) => _ = SetSheetDisc(sheetFileId, disc),
        OpenDocument: (name, path) =>
            _ = _dialogs.ShowDocumentFile(new ImportDocument { Name = name, Path = path }),
        OpenImages: ShowFolderImages,
        Preview: path => _app.Playback.PreviewPlay(path),
        StopPreview: () => _app.Playback.PreviewStop(),
        EditTrack: track => _ = EditTrack(track),
        SetTrackArtists: (trackIds, assignments) =>
            _ = SetTrackArtists(trackIds, assignments),
        ChooseFile: (trackId, audio) => _ = ChooseFile(trackId, audio),
        Drop: trackId => _ = Drop(trackId),
        Exclude: fileId => _ = Exclude(fileId));

    // Put a file in a role, or put it back. Core persists it and drops the
    // candidate's stored identify verdict; a file that has changed jobs is a
    // different set of rows, and the per-candidate read draws them.
    private async Task SetRole(string fileId, BridgeFileRoleChoice choice)
    {
        if (_key is { } key)
        {
            await _import.SetFileRole(key, fileId, choice);
        }
    }

    // A sheet binding changes what the folder's audio *is*: one container
    // becomes a dozen entries. The rows the person was editing are a different
    // set afterwards, which is why core drops their row edits with the binding.
    private async Task SetSheetBinding(string sheetFileId, string? audioFileId)
    {
        if (_key is { } key)
        {
            await _import.SetSheetBinding(key, sheetFileId, audioFileId);
        }
    }

    // Which disc a sheet's entries are re-shapes the tracklist exactly as a
    // binding does.
    private async Task SetSheetDisc(string sheetFileId, BridgeSheetDisc disc)
    {
        if (_key is { } key)
        {
            await _import.SetSheetDisc(key, sheetFileId, disc);
        }
    }

    // Store a row's edited track. Core keys it by the row's own identity, so
    // the table it lands on is the one the person was looking at.
    private async Task EditTrack(BridgeRawTrackEdit track)
    {
        if (_key is { } key)
        {
            await _import.SetCandidateTrackEdit(key, track);
        }
    }

    // Apply one row's artist assignments to the selected rows atomically.
    private async Task SetTrackArtists(
        IReadOnlyList<string> trackIds,
        BridgeTrackArtistAssignments assignments)
    {
        if (_key is { } key)
        {
            await _import.SetCandidateTrackArtists(key, trackIds, assignments);
        }
    }

    // Point a row at one of the folder's audio units. The row starts writing
    // that audio because the editor is what says which audio a track's samples
    // come from — core's reading of the folder produced the row, and this is the
    // user overruling it.
    private async Task ChooseFile(string trackId, BridgeAudioFile audio)
    {
        if (_candidate is not { } candidate)
        {
            return;
        }
        var track = candidate.Mapping.Units()
            .Select(MappingTableReading.Track)
            .OfType<BridgeRawTrackEdit>()
            .FirstOrDefault(row => row.Id == trackId);
        if (track is null)
        {
            BaeDiagnostics.Logger.Warning($"{trackId} is not a row of this mapping table");
            return;
        }
        await EditTrack(track with { File = audio });
    }

    // Drop a row the release names and this folder has nothing for. Nothing on
    // disk changes: the release is simply committed without that track.
    private async Task Drop(string trackId)
    {
        if (_key is { } key)
        {
            await _import.DropCandidateTrack(key, trackId);
        }
    }

    // Take a file out of the tracklist. Core persists the decision — it is a
    // fact about the folder, so it survives re-picking a release and relaunching
    // — and its rows leave because the folder they described is a different set
    // now, which is core's answer and not this pane's edit.
    private async Task Exclude(string fileId) =>
        await SetRole(fileId, BridgeFileRoleChoice.NotATrack);

    // The gallery's images in the lightbox, at the one that was clicked.
    private void ShowFolderImages(IReadOnlyList<BridgeMappingImage> images, string path) =>
        _dialogs.ShowFolderImages(
            images
                .Select(image => new LocalArtwork { FileId = image.FileId, Path = image.LocalPath })
                .ToList(),
            path);
}
