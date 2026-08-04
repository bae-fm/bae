using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
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

    // The candidate under the pane, and what the pane last read about it.
    private string? _key;
    private ImportCandidate? _candidate;

    // What the folder is being read as, and the table that reading produces.
    // The table is there from the moment a candidate is selected: before a pick
    // every audio row simply says what it is waiting for.
    private ImportIdentity _identity = ImportIdentity.Release;
    private BridgeMappingTable? _mapping;

    // What picking a release produced: the seed, the claim and the cover
    // choices. Null while nothing is settled, which is also when there is
    // nothing to commit.
    private PrefetchedEdit? _prefetch;
    private BridgeLibraryStatus? _libraryStatus;

    // The release the mapping was computed against, held here rather than looked
    // up again: putting a file back or binding a sheet clears the candidate's
    // stored identify verdict, so by the time the pane needs to recompute the
    // mapping the folder no longer names the release it is already mapped onto.
    // It outlives a switch to Unknown, which is what switching back re-picks.
    private string? _releaseId;
    private BridgeMetadataSource _releaseSource;
    // How far the claim on that release reaches. Held with the release for the
    // same reason: every re-pick sends it back, so a claim the user lowered is
    // not reset by switching to the folder's own tags and back.
    private BridgeClaimLevel _releaseClaim = BridgeClaimLevel.Exact;

    // Whether a read is in flight, and whether the last one for this candidate
    // failed. Both exist for the Ready seed: it fires off store ticks as well as
    // row clicks, so it has to see the read it already started and the one that
    // already failed.
    private bool _prefetching;
    private bool _prefetchFailed;

    // The album-level fields being committed. The tracks are the mapping table's
    // rows, which core reads back out in commit order.
    private BridgeRawPressingEdit _pressing = Empty();
    private string _albumTitle = string.Empty;
    private string _albumArtistText = string.Empty;

    private PickedCover? _cover;

    // The search editor, held open across rebuilds along with what has been
    // typed into it: the pane re-renders whenever a result lands, and a query
    // that reset itself each time would be unusable.
    private bool _searchOpen;
    private List<ReleaseCandidateChoice> _searchResults = new();
    private string _searchArtist = string.Empty;
    private string _searchAlbum = string.Empty;
    private int _searchSource;
    private string _searchError = string.Empty;

    // Storage: managed (remote) against keep-local, and the pin that rides with
    // a remote import. Only offered when the library has a cloud home.
    private bool _storageRemote = true;
    private bool _storagePinned = true;

    private ImportMappingTable? _table;
    private readonly TextBlock _commitError;

    internal ImportMappingPane(AppService app, ImportDialogs dialogs)
    {
        _app = app;
        _import = app.ImportStore;
        _dialogs = dialogs;
        _commitError = DialogUi.Danger();

        Content = _content;
        _import.PreviewElapsedChanged += OnPreviewChanged;
        _import.Changed += OnQueueChanged;
        Render();
    }

    /// <summary>Put a candidate under the pane. Reads the folder's mapping
    /// fresh, so the table is there before anything is picked, and prefetches
    /// the release a Ready row settled on, so its BECOMES column arrives filled;
    /// every other row lands on the search editor, where picking a release is
    /// one click.</summary>
    internal async Task ShowCandidate(BridgeTriageRow row)
    {
        StopPreview();
        _key = row.CandidateKey;
        _candidate = _import.Candidate(row.CandidateKey);
        ResetEdit();
        ReadCandidateMapping();
        Render();

        if (PickedResume.From(row, SeedState) is not null)
        {
            await RefreshDecidedIdentity();
            return;
        }
        // Every other row lands on the search editor, seeded with the matches
        // identify did find: a Needs-you row is asking which pressing, and a
        // Done or Skipped row is not asking anything.
        _searchOpen = true;
        Render();

        // An idle identify state under an answered row means the session's
        // runtime hasn't seen this candidate yet — its answer lives in the
        // stored verdict. Asking core to identify resumes that verdict
        // instantly with no network (an unanswered folder starts a real run),
        // and the matches arrive on the queue tick the broadcast causes.
        if (_candidate is { RowStatus.Kind: "" })
        {
            _ = _import.AutoIdentify(row.CandidateKey);
        }
    }

    private MappingPaneSeedState SeedState =>
        new(_prefetch is not null, _prefetching, _prefetchFailed);

    // The verdict can settle after the pane is already showing the folder — a
    // row identified while it sits under the pane arrives as a queue tick, not
    // as a click — so the same seed runs from here. Everything that would make
    // a seed wrong is in the guard, so a tick that changes nothing about this
    // candidate does nothing.
    private void OnQueueChanged()
    {
        if (_key is not { } key)
        {
            return;
        }
        if (PickedResume.From(TriageListModel.Row(_import.TriageQueue, key), SeedState)
            is not null)
        {
            _ = RefreshDecidedIdentity();
            return;
        }
        // The identify answer for the folder under the pane can land after it
        // — resumed from a stored verdict, or settled by a live run — and it
        // arrives as a queue tick, not as a click. Re-read the candidate so
        // the search editor offers the current matches. Re-rendered only when
        // the answer actually changed: a re-render rebuilds the editor's
        // controls, and doing that on every tick would drop focus mid-typing.
        if (_prefetch is not null || _prefetching)
        {
            return;
        }
        var refreshed = _import.Candidate(key);
        if (IdentifyFingerprint(refreshed) == IdentifyFingerprint(_candidate))
        {
            return;
        }
        _candidate = refreshed;
        Render();
    }

    private static string IdentifyFingerprint(ImportCandidate? candidate) =>
        candidate is null
            ? string.Empty
            : candidate.RowStatus.Kind + "|"
                + string.Join(",", candidate.Matches.Select(choice => choice.ReleaseId));

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
        ResetEdit();
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

    private void ResetEdit()
    {
        _identity = ImportIdentity.Release;
        _mapping = null;
        _prefetch = null;
        _releaseId = null;
        _releaseClaim = BridgeClaimLevel.Exact;
        _prefetching = false;
        _prefetchFailed = false;
        _libraryStatus = null;
        _pressing = Empty();
        _albumTitle = string.Empty;
        _albumArtistText = string.Empty;
        _cover = null;
        _searchOpen = false;
        _searchResults = new List<ReleaseCandidateChoice>();
        _searchArtist = string.Empty;
        _searchAlbum = _candidate?.Name ?? string.Empty;
        _searchSource = 0;
        _searchError = string.Empty;
        _commitError.IsVisible = false;
    }

    private static BridgeRawPressingEdit Empty() => new(
        string.Empty, string.Empty, string.Empty, string.Empty, string.Empty, string.Empty);

    // ── Reading the folder and the release ───────────────────────────────────

    // The table for a folder nobody has picked a release for: every source unit
    // it offers, with what each becomes left open.
    private void ReadCandidateMapping()
    {
        if (_key is not { } key)
        {
            return;
        }
        if (_import.CandidateMapping(key) is { } mapping)
        {
            _mapping = mapping;
        }
    }

    // Picking a release — or the folder's own tags — recomputes the whole
    // mapping against the decided tracklist and fills the table's BECOMES
    // column live: the pane stays open, the candidate stays selected, and
    // nothing is replaced by a placeholder.

    /// <summary>Decide the candidate's identity: core persists the choice and
    /// returns the same seeded edit a later selection's query serves, so a
    /// fresh launch renders exactly what this click rendered.</summary>
    private Task DecideIdentity(BridgeIdentityPick pick) =>
        ApplyIdentity(async key =>
        {
            var (current, result) = await _app.Import.PickCandidateIdentity(key, pick);
            return (current, result.Decided, false, result.Error);
        });

    /// <summary>Re-apply the identity already decided for the candidate — a
    /// selection finding a stored decision, or a shape change re-deriving
    /// under one. Nothing here re-persists: the decision is already stored,
    /// which is where it came from.</summary>
    private Task RefreshDecidedIdentity() =>
        ApplyIdentity(async key =>
        {
            var (current, result) = await _app.Import.CandidateDecidedIdentity(key);
            return (current, result.Decided, result.Undecided, result.Error);
        });

    private async Task ApplyIdentity(
        Func<string, Task<(bool Current, DecidedEdit? Decided, bool Undecided, string? Error)>> operation)
    {
        if (_key is not { } key || _candidate is null)
        {
            return;
        }
        _prefetching = true;
        _prefetchFailed = false;
        Render();
        try
        {
            var (current, decided, undecided, error) = await operation(key);
            if (!current)
            {
                return;
            }
            if (undecided)
            {
                // The stored decision vanished between the row and the read —
                // an edit raced it. Stay undecided; the next tick re-asks.
                return;
            }
            if (decided is null)
            {
                _prefetchFailed = true;
                _app.ShowError(Loc.Chrome("import.error.load_release"), error ?? Loc.Chrome("import.failed"));
                return;
            }

            if (decided.Release is { } release)
            {
                // A cover picked for one release does not carry to another;
                // recomputing the mapping against the same release keeps it.
                if (_releaseId != release.ReleaseId)
                {
                    _cover = null;
                }
                Seed(decided.Edit);
                _identity = ImportIdentity.Release;
                _releaseId = release.ReleaseId;
                _releaseSource = release.Source;
                _releaseClaim = release.Claim;
                _searchOpen = false;
                Render();

                // Advisory, not a gate — a failed check leaves the banner absent.
                var (statusCurrent, status) = await _app.Import.CheckReleaseInLibrary(release.ReleaseId);
                if (statusCurrent && status.Status is { ReleaseInLibrary: true } inLibrary)
                {
                    _libraryStatus = inLibrary;
                    Render();
                }
            }
            else
            {
                Seed(decided.Edit);
                _identity = ImportIdentity.Unknown;
                _libraryStatus = null;
                _searchOpen = false;
                Render();
            }
        }
        finally
        {
            _prefetching = false;
            Render();
        }
    }

    private void Seed(PrefetchedEdit prefetched)
    {
        _prefetch = prefetched;
        _albumTitle = prefetched.Edit.AlbumTitle;
        _albumArtistText = prefetched.Edit.AlbumArtistText;
        _pressing = prefetched.Edit.Pressing;
        _mapping = prefetched.Mapping;
        _commitError.IsVisible = false;
    }

    // Read the folder's mapping again for whatever it is being read as. A
    // binding, a disc assignment or a role change re-shapes the tracklist, so
    // what comes back is for a different set of rows than the one the user was
    // editing — which is exactly why it replaces them.
    private Task Reprefetch()
    {
        if (_identity == ImportIdentity.Unknown || _releaseId is not null)
        {
            return RefreshDecidedIdentity();
        }
        ReadCandidateMapping();
        Render();
        return Task.CompletedTask;
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
        if (_releaseId is { } releaseId)
        {
            await DecideIdentity(new BridgeIdentityPick.Release(_releaseSource, releaseId, _releaseClaim));
            return;
        }
        _identity = ImportIdentity.Release;
        _searchOpen = true;
        Render();
    }

    /// <summary>Claim the picked release at <paramref name="level"/>. It
    /// re-picks the same release, which is what stores the level: the claim is
    /// part of the decision, not a second thing to persist.</summary>
    private async Task SetClaimLevel(BridgeClaimLevel level)
    {
        if (_releaseId is not { } releaseId)
        {
            // The claim line the control lives in is only drawn for a picked
            // release, so there is nothing to claim without one.
            BaeDiagnostics.Logger.Warning("no release picked; nothing to claim");
            return;
        }
        await DecideIdentity(new BridgeIdentityPick.Release(_releaseSource, releaseId, level));
    }

    private void OnPreviewChanged() => _table?.ApplyPreviewAccent();

    // ── Render ───────────────────────────────────────────────────────────────

    private void Render()
    {
        if (_key is null || _candidate is null)
        {
            _content.Content = EmptyState();
            return;
        }

        var sections = new StackPanel { Spacing = 18, Margin = new Thickness(20, 16, 20, 16) };
        sections.Children.Add(ImportPaneUi.ZoneTitle(Loc.Core("ui.import.identity.title")));
        sections.Children.Add(BuildIdentity().Build());
        if (_searchOpen)
        {
            sections.Children.Add(BuildSearchEditor());
        }
        if (_libraryStatus is { } status)
        {
            sections.Children.Add(BuildLibraryStatusBanner(status));
        }
        _table = null;
        if (_mapping is { } mapping)
        {
            _table = new ImportMappingTable(
                mapping,
                sheetFileId => _import.SheetBindingOptions(_key!, sheetFileId),
                () => _import.PreviewingPath,
                (image, path) => _app.Images.Bind(
                    image, new ImageContent.LocalFile(path), ImageWidths.PickerTile),
                MappingActions());
            var table = _table.Build();
            sections.Children.Add(_table.Title());
            sections.Children.Add(table);
        }

        _content.Content = new ScrollViewer { Content = sections };
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

    // ── Section 1: identity ──────────────────────────────────────────────────

    private ImportIdentitySection BuildIdentity() => new()
    {
        Identity = _identity,
        FolderName = _candidate?.Name ?? string.Empty,
        FormatLabel = _candidate?.Files?.FormatLabel ?? string.Empty,
        HasSettled = _prefetch is not null || _identity == ImportIdentity.Unknown,
        CommitRow = _prefetch is null ? null : BuildCommitRow(),
        Title = _albumTitle.Length > 0 ? _albumTitle : _candidate?.Name ?? string.Empty,
        AlbumTitle = _albumTitle,
        AlbumArtistText = _albumArtistText,
        MetaLine = MetaLine(),
        Claim = _prefetch?.Claim,
        HasPick = _releaseId is not null,
        IsReading = _prefetching,
        LoadCover = CoverFace() is { } face
            ? image => _app.Images.Bind(
                image, ImportDialogs.CoverFaceContent(face.IsLocal, face.Source), ImageWidths.PickerTile)
            : null,
        HasCoverOptions = _prefetch is not null,
        Pressing = _prefetch is null ? null : _pressing,
        OnSetIdentity = identity => _ = SetIdentity(identity),
        OnSetClaimLevel = level => _ = SetClaimLevel(level),
        OnFindRelease = () => { _searchOpen = true; Render(); },
        OnEditCover = () => _ = ChooseCover(),
        OnAlbumTitle = value => _albumTitle = value,
        OnAlbumArtist = value => _albumArtistText = value,
        OnPressing = value => _pressing = value,
    };

    /// <summary>"CD · 1996 · 9 tracks", from the live edit and the live table so
    /// it tracks what is being committed. Empty pressing fields drop out rather
    /// than leaving stray separators, and reading the folder as Unknown says so
    /// where a pressing would be.</summary>
    private string MetaLine()
    {
        if (_mapping is not { } mapping)
        {
            return _candidate?.Files?.FormatLabel ?? string.Empty;
        }
        var lead = _identity == ImportIdentity.Unknown
            ? new[] { Loc.Core("ui.import.identity.from_file_tags") }
            : new[] { _pressing.Format, _pressing.Year };
        return string.Join(
            "  ·  ",
            lead
                .Append(Loc.Chrome("import.candidate.tracks", "count", mapping.WillWriteCount()))
                .Where(part => part.Length > 0));
    }

    // What the card shows now: the cover the user picked, else the release's
    // first remote cover, else the folder's first image — which is the order the
    // import's own default follows when nothing is picked.
    private PickedCover? CoverFace() =>
        _cover
        ?? (_prefetch?.RemoteCovers.FirstOrDefault() is { } remote
            ? new PickedCover(
                ReleaseEditorService.RemoteCoverSelection(remote),
                false,
                ReleaseEditorService.RemoteCoverThumbnailUrl(remote))
            : _prefetch?.LocalArtwork.FirstOrDefault() is { } art
                ? new PickedCover(new BridgeCoverSelection.ReleaseImage(art.FileId), true, art.Path)
                : null);

    private async Task ChooseCover()
    {
        if (_prefetch is not { } prefetch)
        {
            return;
        }
        await _dialogs.ShowCoverPicker(prefetch.RemoteCovers, prefetch.LocalArtwork, picked => _cover = picked);
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

    // Search is the identity section's editor, opened from its change control —
    // not a pane mounted alongside it. It starts on whatever the identify
    // pipeline already found for this folder, so the common case is one click.
    private Control BuildSearchEditor()
    {
        var column = new StackPanel { Spacing = 8 };
        column.Children.Add(ImportPaneUi.ZoneTitle(Loc.Core("ui.import.header.find_release")));

        var results = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 190 };
        var choices = _searchResults.Count > 0 ? _searchResults : _candidate?.Matches ?? new List<ReleaseCandidateChoice>();
        results.ItemsSource = choices.Select(choice => choice.Summary).ToList();
        results.SelectionChanged += async (_, _) =>
        {
            if (results.SelectedIndex < 0 || results.SelectedIndex >= choices.Count)
            {
                return;
            }
            var chosen = choices[results.SelectedIndex];
            await DecideIdentity(
                new BridgeIdentityPick.Release(chosen.Source, chosen.ReleaseId, BridgeClaimLevel.Exact));
        };
        column.Children.Add(results);

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

        var cancel = ImportPaneUi.RowButton(Loc.Chrome("action.cancel"));
        cancel.IsEnabled = _prefetch is not null;
        cancel.Click += (_, _) => { _searchOpen = false; Render(); };

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(search);
        actions.Children.Add(cancel);
        column.Children.Add(actions);

        if (_searchError.Length > 0)
        {
            column.Children.Add(ImportPaneUi.Cell(_searchError, secondary: true));
        }
        return column;
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
        EditTrack: EditTrack,
        ChooseFile: ChooseFile,
        Drop: Drop,
        Exclude: fileId => _ = Exclude(fileId));

    // Put a file in a role, or put it back. Core persists it and drops the
    // candidate's stored identify verdict; the table is re-read because a file
    // that has changed jobs is a different set of rows, and there is no row to
    // put back from here.
    private async Task SetRole(string fileId, BridgeFileRoleChoice choice)
    {
        if (_key is not { } key || !await _import.SetFileRole(key, fileId, choice))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        await Reprefetch();
    }

    // A sheet binding changes what the folder's audio *is*: one container
    // becomes a dozen entries. The whole mapping is recomputed from scratch.
    private async Task SetSheetBinding(string sheetFileId, string? audioFileId)
    {
        if (_key is not { } key || !await _import.SetSheetBinding(key, sheetFileId, audioFileId))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        await Reprefetch();
    }

    // Which disc a sheet's entries are re-shapes the tracklist exactly as a
    // binding does, so the table is re-read the same way.
    private async Task SetSheetDisc(string sheetFileId, BridgeSheetDisc disc)
    {
        if (_key is not { } key || !await _import.SetSheetDisc(key, sheetFileId, disc))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        await Reprefetch();
    }

    // Write a row's edited track back onto the row that commits it. Which row a
    // track edits is core's, so the table is handed back the edited row rather
    // than the pane finding a position for it — and nothing re-renders, so the
    // field being typed in keeps its focus and its caret.
    private void EditTrack(BridgeRawTrackEdit track)
    {
        if (_mapping is not { } mapping)
        {
            return;
        }
        _mapping = BaeBridgeMethods.BridgeMappingWithTrack(mapping, track);
        RefreshCommitCounts();
    }

    // Point a row at one of the folder's audio units. The row starts writing
    // that audio because the editor is what says which audio a track's samples
    // come from — core's reading of the folder produced the row, and this is the
    // user overruling it.
    private void ChooseFile(string trackId, BridgeAudioFile audio)
    {
        if (_mapping is not { } mapping)
        {
            return;
        }
        var track = mapping.Units()
            .Select(MappingTableReading.Track)
            .OfType<BridgeRawTrackEdit>()
            .FirstOrDefault(candidate => candidate.Id == trackId);
        if (track is null)
        {
            BaeDiagnostics.Logger.Warning($"{trackId} is not a row of this mapping table");
            return;
        }
        _mapping = BaeBridgeMethods.BridgeMappingWithTrack(mapping, track with { File = audio });
        Render();
    }

    // Drop a row the release names and this folder has nothing for. Nothing is
    // persisted: the folder is unchanged, the release is simply committed
    // without that track.
    private void Drop(string trackId)
    {
        if (_mapping is not { } mapping)
        {
            return;
        }
        _mapping = BaeBridgeMethods.BridgeMappingWithoutTrack(mapping, trackId);
        Render();
    }

    // Take a file out of the tracklist. Core persists the decision — it is a
    // fact about the folder, so it survives re-picking a release and relaunching
    // — and the table drops the file's rows here, because the only other way to
    // refresh it is another read from core, which would discard the user's
    // edits. One container backs every entry of the sheet bound to it, so that
    // sheet's whole group leaves with it; core decides that, not this.
    private async Task Exclude(string fileId)
    {
        if (_key is not { } key || _mapping is not { } mapping)
        {
            return;
        }
        if (!await _import.SetFileRole(key, fileId, BridgeFileRoleChoice.NotATrack))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        _mapping = BaeBridgeMethods.BridgeMappingWithoutFile(mapping, fileId);
        Render();
    }

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
            var remote = new CheckBox { Content = Loc.Chrome("import.storage.managed"), IsChecked = _storageRemote };
            var pinned = new CheckBox
            {
                Content = Loc.Chrome("import.storage.keep_local"),
                IsChecked = _storagePinned,
                IsVisible = _storageRemote,
            };
            remote.IsCheckedChanged += (_, _) =>
            {
                _storageRemote = remote.IsChecked == true;
                pinned.IsVisible = _storageRemote;
            };
            pinned.IsCheckedChanged += (_, _) => _storagePinned = pinned.IsChecked == true;
            storage.Children.Add(remote);
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
        column.Children.Add(_commitError);
        return column;
    }

    // What is still unanswered, restated on every keystroke, because a title
    // typed into an unmatched row is what moves the number.
    private void RefreshCommitCounts()
    {
        if (_mapping is not { } mapping || _unansweredText is null)
        {
            return;
        }
        var unanswered = mapping.UnansweredCount();
        _unansweredText.Text = unanswered == 0
            ? string.Empty
            : Loc.Core("ui.import.commit.unanswered", "count", (long)unanswered);
        _unansweredText.IsVisible = unanswered > 0;
    }

    private async Task Commit()
    {
        if (_key is not { } key || _candidate is not { } candidate || _mapping is not { } mapping)
        {
            return;
        }
        var edit = new BridgeRawReleaseEdit(
            _albumTitle, _albumArtistText, _pressing, BaeBridgeMethods.BridgeMappingTracks(mapping));
        var identity = _prefetch?.Claim?.Choice ?? new BridgeIdentityChoice.Unknown();
        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        var remote = settingsCurrent && settings.HasCloudHome && _storageRemote;

        var (current, error) = await _app.Import.CommitImport(
            key, candidate.FolderPath, identity, remote ? "managed" : "unmanaged",
            remote && _storagePinned, edit, _cover?.Selection);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _commitError.Text = error;
            _commitError.IsVisible = true;
            return;
        }
        Clear();
    }
}
