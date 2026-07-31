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
/// The one surface where a folder becomes a release, to the right of the triage
/// sidebar. Four zones, always in this order, none overlapping: the release
/// header, the file roles, the track slots, the commit bar.
///
/// It replaces the picker/confirm dialog pair the import used to run through —
/// the document list, the mounted search, and the docked confirmation sheet.
/// Search is the header's editor now, opened from its change control; the
/// confirmation sheet's remaining content is the commit bar. Nothing in the
/// commit bar ever disables the commit: a disagreement is stated, and the one
/// refusal left in the whole import is audio that will not decode, which core
/// raises.
///
/// State lives here for one selected candidate. The pane rebuilds on a
/// structural change — a different candidate, a different release, a sheet
/// binding, a role — and never on a store tick, so a field the user is typing
/// in keeps its focus and its caret.
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

    // What picking a release produced: the seed, the claim, the cover choices,
    // and the slot table. Null while nothing is picked.
    private PrefetchedEdit? _prefetch;
    private BridgeLibraryStatus? _libraryStatus;

    // The release the slot table was computed against, held here rather than
    // looked up again: putting a file back or binding a sheet clears the
    // candidate's stored identify verdict, so by the time the pane needs to
    // recompute the mapping the folder no longer names the release it is
    // already mapped onto.
    private string? _releaseId;
    private BridgeMetadataSource _releaseSource;

    // The live edit. `_tracks` is positionally aligned with the model's rows —
    // row i edits track i — and both are re-cut together whenever a row leaves.
    private readonly List<BridgeRawTrackEdit> _tracks = new();
    private MappingPaneModel? _model;
    private ImportSlotTable? _slotTable;
    private BridgeRawPressingEdit _pressing = Empty();
    private string _albumTitle = string.Empty;
    private string _albumArtistText = string.Empty;

    private PickedCover? _cover;

    // The header's editor, held open across rebuilds along with what has been
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

    private readonly TextBlock _commitError;

    internal ImportMappingPane(AppService app, ImportDialogs dialogs)
    {
        _app = app;
        _import = app.ImportStore;
        _dialogs = dialogs;
        _commitError = DialogUi.Danger();

        Content = _content;
        _import.PreviewElapsedChanged += OnPreviewChanged;
        Render();
    }

    /// <summary>Put a candidate under the pane. Reads the folder fresh and
    /// prefetches whichever release the triage row matched, so the slot table
    /// arrives already mapped; a candidate with no match lands on the header's
    /// search editor instead.</summary>
    internal async Task ShowCandidate(BridgeTriageRow row)
    {
        StopPreview();
        _key = row.CandidateKey;
        _candidate = _import.Candidate(row.CandidateKey);
        ResetEdit();
        Render();

        if (row.Matched is { } matched)
        {
            await Prefetch(matched.ReleaseId, matched.Evidence.Source);
            return;
        }
        _searchOpen = true;
        Render();
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
        _prefetch = null;
        _releaseId = null;
        _libraryStatus = null;
        _model = null;
        _slotTable = null;
        _tracks.Clear();
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

    // Picking a release re-runs the prefetch and replaces the slot table's
    // source-side column live: the pane stays open, the candidate stays
    // selected, and the whole mapping is recomputed against the new tracklist.
    private async Task Prefetch(string releaseId, BridgeMetadataSource source)
    {
        if (_key is not { } key || _candidate is not { } candidate)
        {
            return;
        }
        var (current, result) = await _app.Import.PrefetchCandidateEdit(
            key, releaseId, source, candidate.FolderPath);
        if (!current)
        {
            return;
        }
        if (result.Prefetched is not { } prefetched)
        {
            _app.ShowError(Loc.Chrome("import.error.load_release"), result.Error ?? Loc.Chrome("import.failed"));
            return;
        }

        // A cover picked for one release does not carry to another; recomputing
        // the mapping against the same release keeps it.
        if (_releaseId != releaseId)
        {
            _cover = null;
        }
        Seed(prefetched);
        _releaseId = releaseId;
        _releaseSource = source;
        _model = prefetched.Slots is { } slots
            ? MappingPaneProjection.FromSlots(slots, _tracks)
            : MappingPaneProjection.FromUnidentifiedEdit(_tracks);
        _searchOpen = false;
        Render();

        // Advisory, not a gate — a failed check leaves the banner absent.
        var (statusCurrent, status) = await _app.Import.CheckReleaseInLibrary(releaseId);
        if (statusCurrent && status.Status is { ReleaseInLibrary: true } inLibrary)
        {
            _libraryStatus = inLibrary;
            Render();
        }
    }

    // The import that claims nothing: bae-core projects the folder's own file
    // tags into an edit, and computes no slot table for it because there is no
    // release to map against. The rows are the edit's tracks, and they still
    // write.
    private async Task PrefetchUnidentified()
    {
        if (_candidate is not { } candidate)
        {
            return;
        }
        var (current, result) = await _app.Import.PrefetchUnknownEdit(candidate.Key);
        if (!current)
        {
            return;
        }
        if (result.Prefetched is not { } prefetched)
        {
            _app.ShowError(Loc.Chrome("import.error.load_release"), result.Error ?? Loc.Chrome("import.failed"));
            return;
        }
        Seed(prefetched);
        _releaseId = null;
        _libraryStatus = null;
        _model = MappingPaneProjection.FromUnidentifiedEdit(_tracks);
        _searchOpen = false;
        Render();
    }

    private void Seed(PrefetchedEdit prefetched)
    {
        _prefetch = prefetched;
        _albumTitle = prefetched.Edit.AlbumTitle;
        _albumArtistText = prefetched.Edit.AlbumArtistText;
        _pressing = prefetched.Edit.Pressing;
        _tracks.Clear();
        _tracks.AddRange(prefetched.Edit.Tracks);
        _commitError.IsVisible = false;
    }

    private void OnPreviewChanged() => _slotTable?.ApplyPreviewAccent();

    // ── Render ───────────────────────────────────────────────────────────────

    private void Render()
    {
        if (_key is null || _candidate is null)
        {
            _content.Content = EmptyState();
            return;
        }

        var zones = new StackPanel { Spacing = 0, Margin = new Thickness(20, 16, 20, 16) };
        zones.Children.Add(BuildHeader());
        if (_candidate.Files is { } files)
        {
            zones.Children.Add(ImportPaneUi.Divider());
            zones.Children.Add(BuildRoles(files));
        }
        if (_model is { } model)
        {
            zones.Children.Add(ImportPaneUi.Divider());
            zones.Children.Add(BuildSlots(model));
        }

        var scroller = new ScrollViewer { Content = zones };
        var column = new Grid { RowDefinitions = new RowDefinitions("*,Auto") };
        Grid.SetRow(scroller, 0);
        column.Children.Add(scroller);
        if (_model is not null)
        {
            var bar = BuildCommitBar();
            Grid.SetRow(bar, 1);
            column.Children.Add(bar);
        }
        _content.Content = column;
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

    // ── Zone 1: the release header ───────────────────────────────────────────

    private Control BuildHeader()
    {
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("96,*"), ColumnSpacing = 14 };

        var cover = BuildCoverTile();
        Grid.SetColumn(cover, 0);
        grid.Children.Add(cover);

        var body = _searchOpen ? BuildSearchEditor() : BuildHeaderFacts();
        Grid.SetColumn(body, 1);
        grid.Children.Add(body);
        return grid;
    }

    private Control BuildCoverTile()
    {
        var image = new Image { Width = 96, Height = 96, Stretch = Stretch.UniformToFill };
        var tile = new Border
        {
            Width = 96,
            Height = 96,
            CornerRadius = new CornerRadius(8),
            ClipToBounds = true,
            Child = image,
            VerticalAlignment = VerticalAlignment.Top,
        };
        tile[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        LoadCoverPreview(image);

        var change = ImportPaneUi.RowButton(Loc.Chrome("cover.change_title"));
        change.HorizontalAlignment = HorizontalAlignment.Center;
        change.IsEnabled = _prefetch is not null;
        change.Click += async (_, _) => await ChooseCover();

        var column = new StackPanel { Spacing = 6, VerticalAlignment = VerticalAlignment.Top };
        column.Children.Add(tile);
        column.Children.Add(change);
        return column;
    }

    // What the header shows now: the cover the user picked, else the release's
    // first remote cover, else the folder's first image — which is the order the
    // import's own default follows when nothing is picked.
    private void LoadCoverPreview(Image image)
    {
        var face = _cover
            ?? (_prefetch?.RemoteCovers.FirstOrDefault() is { } remote
                ? new PickedCover(
                    ReleaseEditorService.RemoteCoverSelection(remote),
                    false,
                    ReleaseEditorService.RemoteCoverThumbnailUrl(remote))
                : _prefetch?.LocalArtwork.FirstOrDefault() is { } art
                    ? new PickedCover(new BridgeCoverSelection.ReleaseImage(art.FileId), true, art.Path)
                    : null);
        if (face is null)
        {
            return;
        }
        _app.Images.Bind(
            image,
            ImportDialogs.CoverFaceContent(face.IsLocal, face.Source),
            ImageWidths.PickerTile);
    }

    private async Task ChooseCover()
    {
        if (_prefetch is not { } prefetch)
        {
            return;
        }
        await _dialogs.ShowCoverPicker(prefetch.RemoteCovers, prefetch.LocalArtwork, picked => _cover = picked);
        Render();
    }

    // The release the folder is being mapped onto: its title and artist, both
    // editable, the claim line stating what the import records, and the control
    // that opens the header's editor to change which release it is.
    private Control BuildHeaderFacts()
    {
        var column = new StackPanel { Spacing = 6 };

        var title = new TextBox
        {
            Text = _albumTitle,
            FontSize = 16,
            FontWeight = FontWeight.SemiBold,
            Watermark = Loc.Chrome("edit.field.album_title"),
        };
        title.TextChanged += (_, _) => _albumTitle = title.Text ?? string.Empty;
        column.Children.Add(title);

        var artist = new TextBox
        {
            Text = _albumArtistText,
            FontSize = 13,
            Watermark = Loc.Chrome("edit.field.album_artists"),
        };
        artist.TextChanged += (_, _) => _albumArtistText = artist.Text ?? string.Empty;
        column.Children.Add(artist);

        // Stated, never asked: bae-core derived the claim from the evidence
        // that identified the candidate, and picking a different release in the
        // editor below is what moves it. An import that claims nothing has no
        // source release to name.
        if (_prefetch?.Claim is { } claim)
        {
            column.Children.Add(ClaimLineView.Build(claim));
        }
        else if (_prefetch is not null)
        {
            column.Children.Add(ImportPaneUi.Cell(Loc.Chrome("import.pane.no_release"), secondary: true));
        }

        if (_libraryStatus is { } status)
        {
            column.Children.Add(BuildLibraryStatusBanner(status));
        }

        var change = ImportPaneUi.RowButton(Loc.Core("ui.import.header.change_release"));
        change.HorizontalAlignment = HorizontalAlignment.Left;
        change.Click += (_, _) => { _searchOpen = true; Render(); };

        column.Children.Add(change);

        // The pressing fields, folded away. The claim line above already says
        // which pressing the import records; this is where a wrong year or a
        // missing catalog number gets fixed before it is written.
        if (_prefetch is not null)
        {
            column.Children.Add(new Expander
            {
                Header = Loc.Chrome("import.pane.pressing_details"),
                FontSize = 12,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                Content = BuildPressingFields(),
            });
        }

        return column;
    }

    private Control BuildPressingFields()
    {
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*,*"),
            RowDefinitions = new RowDefinitions("Auto,Auto"),
            ColumnSpacing = 8,
            RowSpacing = 6,
        };
        void Add(int column, int row, string labelKey, string value, Action<string> write)
        {
            var field = DialogUi.Field(Loc.Chrome(labelKey), out var box);
            box.Text = value;
            box.FontSize = 12;
            box.TextChanged += (_, _) => write(box.Text ?? string.Empty);
            Grid.SetColumn(field, column);
            Grid.SetRow(field, row);
            grid.Children.Add(field);
        }
        Add(0, 0, "edit.field.year", _pressing.Year, value => _pressing = _pressing with { Year = value });
        Add(1, 0, "edit.field.format", _pressing.Format, value => _pressing = _pressing with { Format = value });
        Add(2, 0, "edit.field.label", _pressing.Label, value => _pressing = _pressing with { Label = value });
        Add(0, 1, "edit.field.catalog_number", _pressing.CatalogNumber,
            value => _pressing = _pressing with { CatalogNumber = value });
        Add(1, 1, "edit.field.country", _pressing.Country, value => _pressing = _pressing with { Country = value });
        Add(2, 1, "edit.field.barcode", _pressing.Barcode, value => _pressing = _pressing with { Barcode = value });
        return grid;
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

    // Search is the header's editor, opened from its change control — not a
    // pane mounted alongside it. It starts on whatever the identify pipeline
    // already found for this folder, so the common case is one click.
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
            await Prefetch(chosen.ReleaseId, chosen.Source);
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

        // The import that claims nothing: bae-core reads the folder's own file
        // tags rather than a release. Offered here because "no release" is an
        // answer to the same question the search asks.
        var unidentified = ImportPaneUi.RowButton(Loc.Chrome("identify.skip"));
        unidentified.Click += async (_, _) => await PrefetchUnidentified();

        var cancel = ImportPaneUi.RowButton(Loc.Chrome("action.cancel"));
        cancel.IsEnabled = _prefetch is not null;
        cancel.Click += (_, _) => { _searchOpen = false; Render(); };

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(search);
        actions.Children.Add(unidentified);
        actions.Children.Add(cancel);
        column.Children.Add(actions);

        if (_searchError.Length > 0)
        {
            column.Children.Add(ImportPaneUi.Cell(_searchError, secondary: true));
        }
        return column;
    }

    // ── Zone 2: the file roles ───────────────────────────────────────────────

    private Control BuildRoles(BridgeCandidateFiles files) => new ImportRolesTable(
        files,
        SetRole,
        sheetFileId => _import.SheetBindingOptions(_key!, sheetFileId),
        SetSheetBinding,
        document => _ = _dialogs.ShowDocumentFile(document)).Build();

    // Changing a file's role from the roles table. Taking a file out of the
    // tracklist drops the rows it backed, exactly as a slot's Exclude does;
    // putting one back has to re-prefetch, because the row that appears has no
    // source in the table the pane is holding.
    private async Task SetRole(string fileId, BridgeFileRoleChoice choice)
    {
        if (_key is not { } key || !await _import.SetFileRole(key, fileId, choice))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        if (choice == BridgeFileRoleChoice.NotATrack)
        {
            DropRowsBackedBy(fileId);
            Render();
            return;
        }
        await Reprefetch();
    }

    // A sheet binding changes what the folder's audio is, so the slot table is
    // recomputed from scratch — the walkthrough folder goes from one slot to
    // twelve without leaving the pane.
    private async Task SetSheetBinding(string sheetFileId, string? audioFileId)
    {
        if (_key is not { } key || !await _import.SetSheetBinding(key, sheetFileId, audioFileId))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        await Reprefetch();
    }

    // Recompute the whole mapping against the release it is already mapped
    // onto. An import that claims nothing has no release to recompute against,
    // so its rows stay the folder's own tags.
    private async Task Reprefetch()
    {
        if (_releaseId is { } releaseId)
        {
            await Prefetch(releaseId, _releaseSource);
            return;
        }
        if (_prefetch is not null)
        {
            await PrefetchUnidentified();
            return;
        }
        Render();
    }

    // ── Zone 3: the track slots ──────────────────────────────────────────────

    private Control BuildSlots(MappingPaneModel model)
    {
        _slotTable = new ImportSlotTable(
            model,
            _prefetch?.Slots?.Audio ?? Array.Empty<BridgeSlotFile>(),
            onTitle: (index, text) =>
            {
                model.SetTitle(index, text);
                _tracks[index] = _tracks[index] with { Title = text };
                RefreshCommitCounts();
            },
            onArtist: (index, text) =>
            {
                model.SetArtist(index, text);
                _tracks[index] = _tracks[index] with { ArtistText = text };
            },
            onExclude: index => _ = Exclude(index),
            onDrop: index => Drop(index),
            onChooseFile: ChooseFile,
            onPlay: index =>
            {
                if (model.PlayPath(index) is { } path)
                {
                    _app.Playback.PreviewPlay(path);
                }
            },
            onStop: () => _app.Playback.PreviewStop(),
            previewingPath: () => _import.PreviewingPath);
        return _slotTable.Build();
    }

    // Excluding is a fact about the folder, not a list edit: core persists it,
    // so it survives re-picking a release and relaunching. Once the write
    // lands the pane drops every row that file backed — a container backs
    // several — and the edit rows at the same indices. It does not re-prefetch:
    // that would discard the titles the user has typed.
    private async Task Exclude(int index)
    {
        if (_key is not { } key || _model is not { } model || model.Rows[index].FileId is not { } fileId)
        {
            return;
        }
        if (!await _import.SetFileRole(key, fileId, BridgeFileRoleChoice.NotATrack))
        {
            return;
        }
        _candidate = _import.Candidate(key);
        DropRowsBackedBy(fileId);
        Render();
    }

    private void DropRowsBackedBy(string fileId)
    {
        if (_model is not { } model)
        {
            return;
        }
        var removed = model.Exclude(fileId);
        for (var i = removed.Count - 1; i >= 0; i--)
        {
            _tracks.RemoveAt(removed[i]);
        }
    }

    // "Drop" removes an unanswered slot from the edit entirely. It says nothing
    // about the folder, so nothing is persisted.
    private void Drop(int index)
    {
        if (_model is not { } model)
        {
            return;
        }
        model.RemoveAt(index);
        _tracks.RemoveAt(index);
        Render();
    }

    private void ChooseFile(int index, BridgeSlotFile file)
    {
        if (_model is not { } model)
        {
            return;
        }
        model.Pair(
            index,
            MappingPaneProjection.FileId(file.Audio),
            file.Name,
            file.Size,
            file.LocalPath,
            file.ProbedDurationMs,
            MappingSlotSpan.Whole);
        _tracks[index] = _tracks[index] with { File = file.Audio };
        Render();
    }

    // ── Zone 4: the commit bar ───────────────────────────────────────────────

    private TextBlock? _willWriteText;
    private TextBlock? _unansweredText;

    private Control BuildCommitBar()
    {
        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        var hasCloudHome = settingsCurrent && settings.HasCloudHome;

        var counts = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        _willWriteText = ImportPaneUi.Cell(string.Empty);
        _unansweredText = ImportPaneUi.Cell(string.Empty, secondary: true);
        counts.Children.Add(_willWriteText);
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
        import.IsEnabled = MappingPaneModel.CommitEnabled;
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

        var bar = new Border { Padding = new Thickness(20, 12), Child = column, BorderThickness = new Thickness(0, 1, 0, 0) };
        bar[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        bar[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        return bar;
    }

    // What committing now would write, and what is unanswered — both restated
    // on every keystroke, because a title typed into an unmatched row is what
    // moves the second number.
    private void RefreshCommitCounts()
    {
        if (_model is not { } model || _willWriteText is null || _unansweredText is null)
        {
            return;
        }
        _willWriteText.Text = Loc.Core("ui.import.commit.will_write", "count", (long)model.WillWriteCount);
        var unanswered = model.UnansweredCount;
        _unansweredText.Text = unanswered == 0
            ? string.Empty
            : Loc.Core("ui.import.commit.unanswered", "count", (long)unanswered);
        _unansweredText.IsVisible = unanswered > 0;
    }

    private async Task Commit()
    {
        if (_key is not { } key || _candidate is not { } candidate)
        {
            return;
        }
        var edit = new BridgeRawReleaseEdit(_albumTitle, _albumArtistText, _pressing, _tracks.ToArray());
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
