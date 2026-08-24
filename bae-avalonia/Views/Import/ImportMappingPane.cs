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
/// place, and the identity control switches between the release and the folder's
/// own tags without emptying it. Nothing in the commit bar ever disables the
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
internal sealed class ImportMappingPane : UserControl
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
    private BridgeCandidateRuntimeSnapshot? _runtime;

    // Whether a pick is in flight. The control that started it says so; the
    // pane behind it keeps showing whatever is stored until the pick lands.
    private bool _pickInFlight;

    // The search editor, held open across rebuilds along with what has been
    // typed into it: the pane re-renders whenever a result lands, and a query
    // that reset itself each time would be unusable.
    private bool _searchOpen;
    // Which half of the search editor is showing: what identification found,
    // or the typed search. The editor owns it, so opening it again starts on
    // the matches rather than wherever the last session left off.
    private bool _searchManual;
    private List<ReleaseCandidateChoice> _searchResults = new();
    private string _searchArtist = string.Empty;
    private string _searchAlbum = string.Empty;
    private int _searchSource;
    private string _searchError = string.Empty;

    // Storage: cloud against local, and the pin that rides with a cloud import.
    // Only offered when the library has a cloud home.
    private bool _storageCloud = true;
    private bool _storagePinned = true;

    private ImportMappingTable? _table;

    // What the last commit refused with. A string and not the control that
    // shows it: the pane rebuilds its tree on every render, and a control held
    // across renders would be added to a second parent on the next one.
    private string _commitError = string.Empty;

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
        // The subscription is already open, so this read cannot be undone by a
        // change that was on its way.
        _runtime = _import.CandidateRuntime(row.CandidateKey);
        ResetSession();
        ObserveRelease();
        // Nothing picked is the question the search editor answers, so a row
        // that has not been decided lands on it.
        _searchOpen = _candidate is { HasSettled: false };
        Render();

        // An idle identify state on a folder nothing is settled for means the
        // session's runtime hasn't seen this candidate yet — its answer, if it
        // has one, lives in the stored verdict. Asking core to identify
        // resumes that verdict instantly with no network (an unanswered folder
        // starts a real run), and the matches arrive on the queue tick the
        // broadcast causes. A folder that is already picked is not asking.
        if (_runtime is null && _candidate is { RowStatus.Kind: "", HasSettled: false })
        {
            _ = _import.AutoIdentify(row.CandidateKey);
        }
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
        if (Equals(refreshed?.Detail, _candidate?.Detail)
            && IdentifyFingerprint(refreshed) == IdentifyFingerprint(_candidate))
        {
            return;
        }
        var pickChanged = !Equals(refreshed?.PickedRelease, _candidate?.PickedRelease);
        _candidate = refreshed;
        if (pickChanged)
        {
            ObserveRelease();
            _searchOpen = _candidate is { HasSettled: false };
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
        _searchOpen = false;
        _searchManual = false;
        _searchResults = new List<ReleaseCandidateChoice>();
        _searchArtist = string.Empty;
        _searchAlbum = _candidate?.Name ?? string.Empty;
        _searchSource = 0;
        _searchError = string.Empty;
        _commitError = string.Empty;
    }

    // ── Deciding the identity ────────────────────────────────────────────────

    /// <summary>Decide the candidate's identity — a pressing, or the folder's
    /// own tags. Core archives the release's documents, stores the pick, and
    /// the per-candidate read delivers the pane's next value; nothing is
    /// seeded from the call.</summary>
    private async Task DecideIdentity(BridgeIdentityPick pick)
    {
        if (_key is not { } key)
        {
            return;
        }
        _pickInFlight = true;
        Render();
        try
        {
            var (current, error) = await _app.Import.PickCandidateIdentity(key, pick);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                _app.ShowError(Loc.Chrome("import.error.load_release"), error);
            }
        }
        finally
        {
            _pickInFlight = false;
            Render();
        }
    }

    /// <summary>Switch what the folder is read as. Unknown reads its own file
    /// tags; Release re-picks the release the candidate already holds, and opens
    /// the search when it holds none — there is nothing to go back to
    /// then.</summary>
    private async Task SetIdentity(ImportIdentity identity)
    {
        if (identity == ImportIdentity.Unknown)
        {
            await DecideIdentity(new BridgeIdentityPick.Unknown());
            return;
        }
        if (_candidate?.PickedRelease is { } picked)
        {
            await DecideIdentity(
                new BridgeIdentityPick.Release(picked.Source, picked.ReleaseId));
            return;
        }
        _searchOpen = true;
        _searchManual = false;
        Render();
    }

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
        foreach (var boundary in _candidate?.Detail?.Row.ResolvedBoundaries ?? [])
        {
            sections.Children.Add(FolderReadingRow(boundary));
        }
        sections.Children.Add(ImportPaneUi.ZoneTitle(Loc.Core("ui.import.metadata.title")));
        sections.Children.Add(BuildIdentity().Build());
        if (_searchOpen)
        {
            sections.Children.Add(BuildSearchEditor());
        }
        if (_import.ReleaseLibraryStatus is { } status)
        {
            sections.Children.Add(BuildLibraryStatusBanner(status));
        }
        if (_candidate?.Failure is { } failure && EffectiveRowStatus.Kind.Length == 0)
        {
            sections.Children.Add(BuildFailureBanner(failure));
        }
        _table = null;
        if (_candidate?.Detail is not null)
        {
            var mapping = _candidate.Mapping;
            var actions = MappingActions();
            if (mapping.Images.Length > 0)
            {
                sections.Children.Add(new ImportMappingGallery(
                    mapping.Images,
                    (image, path) => _app.Images.Bind(
                        image, new ImageContent.LocalFile(path), ImageWidths.PickerTile),
                    actions.OpenImages).Build());
            }
            _table = new ImportMappingTable(
                mapping,
                sheetFileId => _import.SheetBindingOptions(_key!, sheetFileId),
                () => _import.PreviewingPath,
                actions,
                _candidate.Detail!.Unprobed);
            var table = _table.Build();
            sections.Children.Add(_table.Title());
            sections.Children.Add(table);
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

    // How the folder this candidate came out of was read, and the control that
    // reads it the other way. The scan reads such a folder for itself so the
    // queue has candidates to work on; this is where that reading is visible
    // and where it is overruled.
    private Control FolderReadingRow(BridgeResolvedFolderReleaseBoundary boundary)
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var name = new TextBlock
        {
            Text = boundary.Name,
            FontSize = 11.5,
            VerticalAlignment = VerticalAlignment.Center,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        row.Children.Add(Icons.Glyph(Icons.Folder, 12, "BaeTextSecondaryBrush"));
        row.Children.Add(name);

        var flipped = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
            ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
            : BridgeFolderReleaseDecision.CombineAsOneRelease;
        var flip = ImportPaneUi.RowButton(Loc.Chrome(
            flipped is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? "import.release.one"
                : "import.release.separate"));
        var key = boundary.Key;
        flip.Click += (_, _) => _import.SetFolderReleaseDecision(key, flipped);
        row.Children.Add(flip);
        return row;
    }

    // ── Section 1: metadata ──────────────────────────────────────────────────

    private ImportIdentitySection BuildIdentity() => new()
    {
        Identity = _candidate?.Identity ?? ImportIdentity.Release,
        HasSettled = _candidate?.HasSettled ?? false,
        CommitRow = _candidate is { HasSettled: true } ? BuildCommitRow() : null,
        Title = _candidate?.Edit?.AlbumTitle is { Length: > 0 } title
            ? title
            : _candidate?.Name ?? string.Empty,
        Edit = _candidate?.Edit,
        MetaLine = MetaLine(),
        Evidence = _candidate?.Evidence,
        HasPick = _candidate?.PickedRelease is not null,
        IsReading = _pickInFlight,
        LoadCover = _candidate?.Cover is { } cover
            ? image => _app.Images.Bind(
                image, ImportDialogs.CoverChoiceContent(cover), ImageWidths.PickerTile)
            : null,
        HasCoverOptions = _candidate is { HasSettled: true },
        OnSetIdentity = identity => _ = SetIdentity(identity),
        OnFindRelease = () => { _searchOpen = true; _searchManual = false; Render(); },
        OnEditCover = () => _ = ChooseCover(),
        OnEditField = (field, value) => _ = SetEditField(field, value),
    };

    private async Task SetEditField(BridgeCandidateEditField field, string value)
    {
        if (_key is { } key)
        {
            await _import.SetCandidateEditField(key, field, value);
        }
    }

    /// <summary>"CD · 1996 · XE · 463360 2 · 10 tracks", from the live edit and
    /// the live table so it tracks what is being committed. Empty pressing
    /// fields drop out rather than leaving stray separators, and reading the
    /// folder as Unknown says so where a pressing would be.</summary>
    private string MetaLine()
    {
        if (_candidate?.Edit is not { } edit)
        {
            return _candidate?.Files?.FormatLabel ?? string.Empty;
        }
        var mapping = _candidate.Mapping;
        var lead = _candidate.Identity == ImportIdentity.Unknown
            ? new[] { Loc.Core("ui.import.metadata.from_file_tags") }
            : new[]
            {
                edit.Pressing.Format,
                edit.Pressing.Year,
                edit.Pressing.Country,
                edit.Pressing.CatalogNumber,
            };
        return string.Join(
            "  ·  ",
            lead
                .Append(Loc.Chrome("import.candidate.tracks", "count", mapping.WillWriteCount()))
                .Where(part => part.Length > 0));
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

    // The already-in-library banner: a warning line, with a jump to the
    // duplicate when its album id is known.
    private Control BuildLibraryStatusBanner(BridgeLibraryStatus status)
    {
        var banner = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10, 6),
            BorderThickness = new Thickness(1),
        };
        banner[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        banner[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        row.Children.Add(ImportPaneUi.Cell(Loc.Chrome("import.already_in_library"), secondary: true));
        if (!string.IsNullOrEmpty(status.AlbumId))
        {
            var view = ImportPaneUi.RowButton(Loc.Chrome("import.view_in_library"));
            var albumId = status.AlbumId;
            view.Click += async (_, _) => await _dialogs.OpenAlbum(albumId);
            row.Children.Add(view);
        }
        banner.Child = row;
        return banner;
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
        actions.Children.Add(CancelButton());
        column.Children.Add(actions);
        return column;
    }

    // The typed search. `Signals` goes back to what identification found;
    // `Auto` goes back and looks the signals up again.
    private Control ManualHalf()
    {
        var column = new StackPanel { Spacing = 8 };

        var back = ImportPaneUi.RowButton(Loc.Chrome("import.search.signals"));
        back.Click += (_, _) => { _searchManual = false; Render(); };
        var auto = ImportPaneUi.RowButton(Loc.Chrome("import.search.auto"));
        auto.Click += (_, _) =>
        {
            _searchManual = false;
            if (_key is { } key)
            {
                _ = _import.AutoIdentify(key);
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
        actions.Children.Add(CancelButton());
        column.Children.Add(actions);

        if (_searchResults.Count > 0)
        {
            column.Children.Add(ChoiceList(_searchResults));
        }
        return column;
    }

    // The pressings on offer, whichever half produced them. Picking one
    // decides the identity, which is what closes the editor.
    private Control ChoiceList(List<ReleaseCandidateChoice> choices)
    {
        var results = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 190 };
        results.ItemsSource = choices.Select(choice => choice.Summary).ToList();
        results.SelectionChanged += async (_, _) =>
        {
            if (results.SelectedIndex < 0 || results.SelectedIndex >= choices.Count)
            {
                return;
            }
            var chosen = choices[results.SelectedIndex];
            await DecideIdentity(
                new BridgeIdentityPick.Release(chosen.Source, chosen.ReleaseId));
        };
        return results;
    }

    private Button CancelButton()
    {
        var cancel = ImportPaneUi.RowButton(Loc.Chrome("action.cancel"));
        cancel.IsEnabled = _candidate is { HasSettled: true };
        cancel.Click += (_, _) => { _searchOpen = false; Render(); };
        return cancel;
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

    // ── The commit row ───────────────────────────────────────────────────────

    private TextBlock? _unansweredText;

    /// <summary>The identity card's foot: what is still unanswered, storage,
    /// and the Import action — the commit lives on the card that states what
    /// will be committed.</summary>
    private Control BuildCommitRow()
    {
        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        var hasCloudHome = settingsCurrent && settings.HasCloudHome;

        var counts = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        _unansweredText = ImportPaneUi.Cell(string.Empty, secondary: true);
        counts.Children.Add(_unansweredText);
        RefreshCommitCounts();

        var storage = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 10,
            VerticalAlignment = VerticalAlignment.Center,
        };
        if (hasCloudHome)
        {
            var cloud = new CheckBox { Content = Loc.Chrome("import.storage.cloud"), IsChecked = _storageCloud };
            var pinned = new CheckBox
            {
                Content = Loc.Chrome("import.storage.pinned"),
                IsChecked = _storagePinned,
                IsVisible = _storageCloud,
            };
            cloud.IsCheckedChanged += (_, _) =>
            {
                _storageCloud = cloud.IsChecked == true;
                pinned.IsVisible = _storageCloud;
            };
            pinned.IsCheckedChanged += (_, _) => _storagePinned = pinned.IsChecked == true;
            storage.Children.Add(cloud);
            storage.Children.Add(pinned);
        }

        // Nothing here disables the commit. The counts are stated; the one
        // refusal left in the whole import is audio that will not decode, and
        // core raises that.
        var import = DialogUi.Primary(Loc.Chrome("action.import"));
        import.Click += async (_, _) => await Commit();

        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto"), ColumnSpacing = 12 };
        Grid.SetColumn(counts, 0);
        Grid.SetColumn(storage, 1);
        Grid.SetColumn(import, 2);
        row.Children.Add(counts);
        row.Children.Add(storage);
        row.Children.Add(import);

        var column = new StackPanel { Spacing = 6 };
        column.Children.Add(row);
        if (_commitError.Length > 0)
        {
            var refusal = DialogUi.Danger();
            refusal.Text = _commitError;
            column.Children.Add(refusal);
        }
        return column;
    }

    // What is still unanswered, restated from the table core answered with.
    private void RefreshCommitCounts()
    {
        if (_candidate?.Detail is null || _unansweredText is null)
        {
            return;
        }
        var mapping = _candidate.Mapping;
        var unanswered = mapping.UnansweredCount();
        _unansweredText.Text = unanswered == 0
            ? string.Empty
            : Loc.Core("ui.import.commit.unanswered", "count", (long)unanswered);
        _unansweredText.IsVisible = unanswered > 0;
    }

    // Commit the candidate. Nothing about the release is sent: the pick, the
    // metadata typed over it, the corrected rows and the chosen cover are all
    // stored under the candidate, so the commit consumes the very values this
    // pane drew.
    private async Task Commit()
    {
        if (_key is not { } key)
        {
            return;
        }
        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        var cloud = settingsCurrent && settings.HasCloudHome && _storageCloud;

        var (current, error) = await _app.Import.CommitImport(
            key, cloud ? "cloud" : "local", cloud && _storagePinned);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _commitError = error;
            Render();
            return;
        }
        Clear();
    }

    // The last import of this candidate that failed, as it survives a relaunch.
    // Shown when nothing is running for the candidate — while an import is
    // under way, its own status is the current answer.
    private Control BuildFailureBanner(BridgeImportFailure failure)
    {
        var banner = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(10, 6),
            BorderThickness = new Thickness(1),
        };
        banner[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        banner[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        row.Children.Add(ImportPaneUi.Cell(failure.Error, secondary: true));
        var retry = ImportPaneUi.RowButton(Loc.Chrome("import.row.retry"));
        retry.Click += async (_, _) => await Commit();
        row.Children.Add(retry);
        banner.Child = row;
        return banner;
    }
}
