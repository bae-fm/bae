using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The import identify + confirm flow for a scanned candidate, presented in the
// window's modal host: pick an identity (the auto-identified matches, or one found
// by manual search), preview the candidate's audio, read its evidence documents,
// then advance to a confirm step that seeds the album/pressing/track edit form,
// picks a cover, chooses a storage mode, and commits. One presenter per main
// window; every read/write runs through a domain service, so no view here touches
// NativeBae. Mirrors the WinUI ImportPickerDialog + ImportConfirmDialog, adapted to
// the modal host (which shows one dialog at a time, so the picker→confirm→back-to-
// search cycle is a sequence of Show calls rather than nested ContentDialogs).
internal sealed class ImportDialogs
{
    private readonly AppService _app;
    private readonly ModalHost _host;
    private readonly LightboxOverlay _lightbox;
    private readonly Func<string, Task> _openAlbum;

    public ImportDialogs(AppService app, ModalHost host, LightboxOverlay lightbox, Func<string, Task> openAlbum)
    {
        _app = app;
        _host = host;
        _lightbox = lightbox;
        _openAlbum = openAlbum;
    }

    private enum PickerOutcome
    {
        Cancel,
        Import,
        Skip,
        Document,
    }

    // The picker's identity results and the picked row survive a back-to-search
    // return, so the cycle re-opens the picker with its search state intact.
    public async Task ShowPicker(ImportCandidate candidate)
    {
        var results = new List<ReleaseCandidateChoice>(candidate.Matches);
        var selectedIndex = -1;
        var outcome = PickerOutcome.Cancel;
        (string Name, string Text)? pendingDocument = null;
        Action? previewHandler = null;

        try
        {
            while (true)
            {
                outcome = PickerOutcome.Cancel;
                pendingDocument = null;

                await _host.Show(close => BuildPicker(
                    candidate,
                    results,
                    selectedIndex,
                    onSelect: index => selectedIndex = index,
                    onResults: fresh => { results = fresh; selectedIndex = -1; },
                    onPreviewHandler: handler => previewHandler = handler,
                    onDocument: doc => { pendingDocument = doc; outcome = PickerOutcome.Document; close(); },
                    onImport: () => { outcome = PickerOutcome.Import; close(); },
                    onSkip: () => { outcome = PickerOutcome.Skip; close(); },
                    onCancel: close));

                if (previewHandler is not null)
                {
                    _app.ImportStore.PreviewElapsedChanged -= previewHandler;
                    previewHandler = null;
                }

                // A document click hid the picker to make room for the viewer; show
                // it, then re-open the picker with its results and selection intact.
                // The audio preview keeps playing underneath.
                if (pendingDocument is { } doc)
                {
                    await ShowDocument(doc.Name, doc.Text);
                    continue;
                }

                _app.Playback.PreviewStop();

                ReleaseCandidateChoice? chosen;
                if (outcome == PickerOutcome.Import)
                {
                    if (selectedIndex < 0 || selectedIndex >= results.Count)
                    {
                        return;
                    }
                    chosen = results[selectedIndex];
                }
                else if (outcome == PickerOutcome.Skip)
                {
                    chosen = null;
                }
                else
                {
                    return;
                }

                var backToSearch = await ShowConfirm(candidate, chosen);
                if (!backToSearch)
                {
                    return;
                }
            }
        }
        finally
        {
            if (previewHandler is not null)
            {
                _app.ImportStore.PreviewElapsedChanged -= previewHandler;
            }
            _app.ImportStore.ClearPreview();
        }
    }

    // One track sheet: what it carves, what it describes, and the picker that
    // names the audio. The choices come from core already filtered to what the
    // sheet can use, each refusal carrying its reason — offering a file that
    // would fail at commit is the failure the editable binding removes.
    private Control TrackSheetBindingRow(string candidateKey, ImportTrackSheet sheet, TextBlock status)
    {
        var row = new StackPanel { Spacing = 4 };
        var line = DialogUi.Body(TrackSheetLine(sheet));
        row.Children.Add(line);

        var combo = new ComboBox
        {
            HorizontalAlignment = HorizontalAlignment.Stretch,
            PlaceholderText = Loc.Core("ui.import.sheet.choose_audio"),
        };
        // Repopulating sets SelectedItem, which raises SelectionChanged; without
        // this the initial fill would read as the user picking what is already
        // bound and write it back.
        var filling = false;
        combo.SelectionChanged += async (_, _) =>
        {
            if (filling || combo.SelectedItem is not ComboBoxItem { Tag: string[] tag })
            {
                return;
            }
            var audioFileId = tag.Length == 0 ? null : tag[0];
            if (audioFileId == sheet.Describes)
            {
                return;
            }
            if (!await _app.ImportStore.SetSheetBinding(candidateKey, sheet.FileId, audioFileId))
            {
                status.IsVisible = true;
                return;
            }
            // Re-read what the folder now is rather than assuming: a binding
            // core refused, or a sheet whose audio has since gone, would leave
            // an optimistic line saying something untrue.
            var reread = _app.ImportStore.Candidate(candidateKey)?.TrackSheets
                .FirstOrDefault(entry => entry.FileId == sheet.FileId);
            if (reread is not null)
            {
                sheet = reread;
                line.Text = TrackSheetLine(sheet);
            }
        };
        row.Children.Add(combo);

        // Core probes every audio file to answer, so the choices are read once,
        // when the picker opens.
        _ = FillSheetBindingOptions(combo, candidateKey, sheet, () => filling = false, () => filling = true);
        return row;
    }

    // A sheet's own line: what it carves once it describes something, and why it
    // describes nothing plus what its directive asked for while it does not.
    private static string TrackSheetLine(ImportTrackSheet sheet) =>
        sheet.Describes is null
            ? string.Join("  ·  ", new[]
            {
                sheet.FileId,
                sheet.UnboundReason,
                sheet.Requested.Count > 0
                    ? Loc.Core("ui.import.sheet.asked_for", "names", string.Join(", ", sheet.Requested))
                    : null,
            }.Where(part => !string.IsNullOrEmpty(part)))
            : $"{sheet.FileId}  ·  {Loc.Chrome("import.candidate.tracks", "count", sheet.TrackCount)}";

    private async Task FillSheetBindingOptions(
        ComboBox combo,
        string candidateKey,
        ImportTrackSheet sheet,
        Action doneFilling,
        Action startFilling)
    {
        var options = await _app.ImportStore.SheetBindingOptions(candidateKey, sheet.FileId);
        startFilling();
        combo.Items.Clear();
        ComboBoxItem? selected = null;
        foreach (var option in options)
        {
            var item = new ComboBoxItem
            {
                Content = option.RefusalReason is null
                    ? option.FileId
                    : $"{option.FileId}  ·  {option.RefusalReason}",
                // A file the sheet cannot use is shown, disabled, with core's
                // reason — a folder whose only audio is unusable reads as "here
                // is why" rather than as an empty list.
                IsEnabled = option.RefusalReason is null,
                Tag = new[] { option.FileId },
            };
            combo.Items.Add(item);
            if (option.FileId == sheet.Describes)
            {
                selected = item;
            }
        }
        if (options.Count > 0)
        {
            var nothing = new ComboBoxItem
            {
                Content = Loc.Core("ui.import.sheet.describes_nothing"),
                Tag = Array.Empty<string>(),
            };
            combo.Items.Add(nothing);
            selected ??= sheet.Describes is null ? nothing : null;
        }
        // Nothing to offer: a sheet naming one file per track, or a folder with
        // no audio. There is no choice to present, so there is no control.
        combo.IsVisible = options.Count > 0;
        combo.SelectedItem = selected;
        doneFilling();
    }

    private Control BuildPicker(
        ImportCandidate candidate,
        List<ReleaseCandidateChoice> results,
        int selectedIndex,
        Action<int> onSelect,
        Action<List<ReleaseCandidateChoice>> onResults,
        Action<Action> onPreviewHandler,
        Action<(string Name, string Text)> onDocument,
        Action onImport,
        Action onSkip,
        Action onCancel)
    {
        var column = DialogUi.Column();
        column.MinWidth = 420;
        column.Children.Add(DialogUi.Title(Loc.Chrome("import.release_title")));
        column.Children.Add(DialogUi.Body(candidate.Name));

        var status = DialogUi.Danger();

        // Preview the candidate's audio before committing to an identity. The live
        // position label follows the import store's preview state while open.
        if (candidate.AudioPaths.Count > 0)
        {
            var play = new Button { Content = "▶ " + Loc.Chrome("import.preview") };
            play.Click += (_, _) => _app.Playback.PreviewPlay(candidate.AudioPaths[0]);
            var pause = new Button { Content = "⏸" };
            pause.Click += (_, _) => _app.Playback.PreviewTogglePause();
            var stop = new Button { Content = "⏹" };
            stop.Click += (_, _) => _app.Playback.PreviewStop();
            var elapsed = new TextBlock { VerticalAlignment = VerticalAlignment.Center, Text = _app.ImportStore.PreviewElapsedText };
            elapsed[!TextBlock.ForegroundProperty] = new Avalonia.Markup.Xaml.MarkupExtensions.DynamicResourceExtension("BaeTextSecondaryBrush");
            void Render() => elapsed.Text = _app.ImportStore.PreviewElapsedText;
            _app.ImportStore.PreviewElapsedChanged += Render;
            onPreviewHandler(Render);
            var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            row.Children.Add(play);
            row.Children.Add(pause);
            row.Children.Add(stop);
            row.Children.Add(elapsed);
            column.Children.Add(row);
        }

        // The candidate's readable evidence files. Clicking one closes the picker to
        // make room for the viewer (one dialog shows at a time) and hands its decoded
        // text back; a failed read renders in the picker's status block instead.
        if (candidate.Documents.Count > 0)
        {
            column.Children.Add(DialogUi.SectionLabel(Loc.Chrome("import.documents")));
            foreach (var document in candidate.Documents)
            {
                var docButton = new Button
                {
                    Content = $"{document.Name}  ·  {Loc.Bytes(document.SizeBytes)}",
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    HorizontalContentAlignment = HorizontalAlignment.Left,
                };
                docButton.Click += (_, _) =>
                {
                    var (text, error) = ImportService.ReadDocumentText(document.Path);
                    if (error is not null || text is null)
                    {
                        status.Text = Loc.Chrome("import.document.read_failed",
                            new Dictionary<string, object?> { ["name"] = document.Name, ["error"] = error ?? string.Empty });
                        status.IsVisible = true;
                        return;
                    }
                    onDocument((document.Name, text));
                };
                column.Children.Add(docButton);
            }
        }

        // The sheet↔audio binding, the one file role a user can edit. Avalonia has
        // no per-file pane yet — that is the mapping-pane task's work — so the
        // control lives beside the documents the picker already lists.
        if (candidate.TrackSheets.Count > 0)
        {
            column.Children.Add(DialogUi.SectionLabel(Loc.Chrome("import.track_sheets")));
            foreach (var sheet in candidate.TrackSheets)
            {
                column.Children.Add(TrackSheetBindingRow(candidate.Key, sheet, status));
            }
        }

        var resultsList = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 240 };
        resultsList.ItemsSource = results.Select(result => result.Summary).ToList();
        resultsList.SelectedIndex = selectedIndex;
        column.Children.Add(resultsList);

        var artistField = DialogUi.Field(Loc.Chrome("import.field.artist_manual"), out var artistBox);
        var albumField = DialogUi.Field(Loc.Chrome("search.field.album"), out var albumBox);
        albumBox.Text = candidate.Name;
        var sourceBox = new ComboBox
        {
            ItemsSource = new[] { "discogs", "musicbrainz" },
            SelectedIndex = 0,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        var sourceField = new StackPanel { Spacing = 4 };
        sourceField.Children.Add(DialogUi.SectionLabel(Loc.Chrome("search.field.source")));
        sourceField.Children.Add(sourceBox);
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        column.Children.Add(artistField);
        column.Children.Add(albumField);
        column.Children.Add(sourceField);
        column.Children.Add(searchButton);
        column.Children.Add(status);

        var import = DialogUi.Primary(Loc.Chrome("action.import"));
        import.IsEnabled = selectedIndex >= 0;
        import.Click += (_, _) => onImport();
        var skip = new Button { Content = Loc.Chrome("identify.skip") };
        skip.Click += (_, _) => onSkip();
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        cancel.Click += (_, _) => onCancel();
        column.Children.Add(DialogUi.Actions(cancel, skip, import));

        resultsList.SelectionChanged += (_, _) =>
        {
            onSelect(resultsList.SelectedIndex);
            import.IsEnabled = resultsList.SelectedIndex >= 0;
        };

        searchButton.Click += async (_, _) =>
        {
            var source = (string)sourceBox.SelectedItem!;
            searchButton.IsEnabled = false;
            var (current, search) = await _app.Import.SearchReleases(source, artistBox.Text ?? string.Empty, albumBox.Text ?? string.Empty);
            searchButton.IsEnabled = true;
            if (!current)
            {
                return;
            }
            if (search.Error is not null)
            {
                status.Text = search.Error;
                status.IsVisible = true;
                return;
            }
            var fresh = search.Candidates ?? new List<ReleaseCandidateChoice>();
            onResults(fresh);
            resultsList.ItemsSource = fresh.Select(result => result.Summary).ToList();
            resultsList.SelectedIndex = -1;
            import.IsEnabled = false;
            status.Text = Loc.Chrome("search.no_matches");
            status.IsVisible = fresh.Count == 0;
        };

        return new ScrollViewer { Content = column, MaxHeight = 560 };
    }

    // The confirm step: seed the edit form from the chosen release (or the folder's
    // file tags for a skip-identify import), let the user revise, pick a cover and
    // storage mode, then commit. Returns true when the user chose "back to search"
    // so the caller re-opens the picker. A null chosen is the skip-identify import.
    private async Task<bool> ShowConfirm(ImportCandidate candidate, ReleaseCandidateChoice? chosen)
    {
        var (current, seed) = chosen is null
            ? await _app.Import.PrefetchUnknownEdit(candidate.FolderPath)
            : await _app.Import.PrefetchCandidateEdit(candidate.Key, chosen.ReleaseId, chosen.Source, candidate.FolderPath);
        if (!current)
        {
            return false;
        }
        if (seed.Prefetched is not { } prefetched)
        {
            await ShowMessage(Loc.Chrome("import.error.load_release"), seed.Error);
            return false;
        }

        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        if (!settingsCurrent)
        {
            return false;
        }

        // Warn when the chosen release is already in the library before importing a
        // duplicate. Advisory, not a gate — a failed check leaves the banner absent.
        BridgeLibraryStatus? libraryStatus = null;
        if (chosen is not null)
        {
            var (statusCurrent, status) = await _app.Import.CheckReleaseInLibrary(chosen.ReleaseId);
            if (!statusCurrent)
            {
                return false;
            }
            if (status.Status is { ReleaseInLibrary: true } inLibrary)
            {
                libraryStatus = inLibrary;
            }
        }

        var backToSearch = false;
        string? viewInLibraryAlbumId = null;

        await _host.Show(close =>
        {
            var form = new ReleaseEditForm(prefetched.Edit, 520);
            var column = DialogUi.Column();
            column.MinWidth = 520;
            column.Children.Add(DialogUi.Title(Loc.Chrome("import.confirm_title")));

            if (libraryStatus is { } status)
            {
                column.Children.Add(BuildLibraryStatusBanner(status, albumId =>
                {
                    viewInLibraryAlbumId = albumId;
                    close();
                }));
            }

            // Storage: managed (remote) vs keep-local, and a pin choice that rides
            // alongside a remote import. Only offered when the library has a cloud home.
            var storageRemote = new CheckBox { Content = Loc.Chrome("import.storage.managed"), IsChecked = true };
            var storagePinned = new CheckBox { Content = Loc.Chrome("import.storage.keep_local"), IsChecked = true };
            bool StorageRemoteSelected() => settings.HasCloudHome && storageRemote.IsChecked == true;
            string StorageModeTag() => StorageRemoteSelected() ? "managed" : "unmanaged";
            bool StoragePinSelected() => StorageRemoteSelected() && storagePinned.IsChecked == true;
            void RefreshStorageControls() => storagePinned.IsVisible = StorageRemoteSelected();
            storageRemote.IsCheckedChanged += (_, _) => RefreshStorageControls();
            RefreshStorageControls();
            if (settings.HasCloudHome)
            {
                var storageRow = new StackPanel
                {
                    Orientation = Orientation.Horizontal,
                    Spacing = 8,
                    HorizontalAlignment = HorizontalAlignment.Right,
                };
                storageRow.Children.Add(storageRemote);
                storageRow.Children.Add(storagePinned);
                column.Children.Add(storageRow);
            }

            // No selection means the import chooses its own default cover.
            BridgeCoverSelection? selectedCover = null;
            column.Children.Add(BuildCoverPicker(prefetched.RemoteCovers, prefetched.LocalArtwork, picked => selectedCover = picked));

            // What the pick claims, and where its metadata came from. Stated,
            // never asked: bae-core derived it from the evidence that identified
            // the candidate, and going back to search to pick a different
            // pressing is what moves it. A skip-identify import claims nothing
            // and has no source release to name.
            if (prefetched.Claim is { } claim)
            {
                column.Children.Add(ClaimLineView.Build(claim));
            }

            column.Children.Add(new ScrollViewer { Content = form.Panel, MaxHeight = 360 });

            var import = DialogUi.Primary(Loc.Chrome("action.import"));
            import.Click += async (_, _) =>
            {
                var payload = form.ReadBack();
                var identityChoice = prefetched.Claim?.Choice ?? new BridgeIdentityChoice.Unknown();
                var (importCurrent, error) = await _app.Import.CommitImport(
                    candidate.Key, candidate.FolderPath, identityChoice, StorageModeTag(), StoragePinSelected(), payload, selectedCover);
                if (!importCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    form.ErrorText.Text = error;
                    form.ErrorText.IsVisible = true;
                    return;
                }
                close();
            };
            var back = new Button { Content = Loc.Chrome("import.back_to_search") };
            back.Click += (_, _) => { backToSearch = true; close(); };
            var cancel = new Button { Content = Loc.Chrome("action.cancel") };
            cancel.Click += (_, _) => close();
            column.Children.Add(DialogUi.Actions(cancel, back, import));

            return new ScrollViewer { Content = column, MaxHeight = 640 };
        });

        // "View in library" closed the dialog; open that album now (the modal host
        // shows one dialog at a time, so it opens after this one is gone).
        if (viewInLibraryAlbumId is not null)
        {
            await _openAlbum(viewInLibraryAlbumId);
            return false;
        }

        return backToSearch;
    }

    // The already-in-library banner: a warning line, with a "view in library" jump
    // when the duplicate's album id is known.
    private static Control BuildLibraryStatusBanner(BridgeLibraryStatus status, Action<string> onView)
    {
        var banner = new Border { CornerRadius = new CornerRadius(8), Padding = new Thickness(12, 8), BorderThickness = new Thickness(1) };
        banner[!Border.BackgroundProperty] = new Avalonia.Markup.Xaml.MarkupExtensions.DynamicResourceExtension("BaeElevatedBrush");
        banner[!Border.BorderBrushProperty] = new Avalonia.Markup.Xaml.MarkupExtensions.DynamicResourceExtension("BaeHairlineBrush");
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12, VerticalAlignment = VerticalAlignment.Center };
        row.Children.Add(DialogUi.Body(Loc.Chrome("import.already_in_library")));
        if (!string.IsNullOrEmpty(status.AlbumId))
        {
            var view = new Button { Content = Loc.Chrome("import.view_in_library") };
            var albumId = status.AlbumId;
            view.Click += (_, _) => onView(albumId);
            row.Children.Add(view);
        }
        banner.Child = row;
        return banner;
    }

    // The cover picker: the chosen release's remote covers (thumbnails by URL) and
    // the candidate folder's local artwork (thumbnails by disk path). Clicking a tile
    // selects it and reports its cover selection; double-tapping local artwork opens
    // the lightbox. Nothing is picked by default, so the import uses its own default.
    private StackPanel BuildCoverPicker(
        List<BridgeRemoteCover> remoteCovers, List<LocalArtwork> localArtwork, Action<BridgeCoverSelection> onPick)
    {
        var section = new StackPanel { Spacing = 4 };
        section.Children.Add(DialogUi.SectionLabel(Loc.Chrome("cover.section_title")));

        if (remoteCovers.Count == 0 && localArtwork.Count == 0)
        {
            section.Children.Add(DialogUi.Body(Loc.Chrome("cover.none_available")));
            return section;
        }

        section.Children.Add(DialogUi.Body(Loc.Chrome("cover.pick_hint")));
        var grid = new WrapPanel { Orientation = Orientation.Horizontal };

        var tiles = new List<Button>();
        void Select(Button picked, BridgeCoverSelection selection)
        {
            foreach (var tile in tiles)
            {
                tile.BorderThickness = new Thickness(0);
            }
            picked.BorderThickness = new Thickness(2);
            picked.BorderBrush = Brushes.DeepSkyBlue;
            onPick(selection);
        }

        Button AddTile(Image image, string caption, BridgeCoverSelection selection)
        {
            var tile = DialogUi.CoverTile(image, caption);
            tile.Click += (_, _) => Select(tile, selection);
            tiles.Add(tile);
            grid.Children.Add(tile);
            return tile;
        }

        foreach (var cover in remoteCovers)
        {
            var image = new Image();
            AddTile(image, cover.Label, ReleaseEditorService.RemoteCoverSelection(cover));
            var url = ReleaseEditorService.RemoteCoverThumbnailUrl(cover);
            _ = CoverImage.LoadUrlAsync(url).ContinueWith(
                task => { if (task.Result is { } bitmap) { image.Source = bitmap; } },
                TaskScheduler.FromCurrentSynchronizationContext());
        }

        for (var i = 0; i < localArtwork.Count; i++)
        {
            var art = localArtwork[i];
            var image = new Image();
            var tile = AddTile(image, System.IO.Path.GetFileName(art.FileId), new BridgeCoverSelection.ReleaseImage(art.FileId));
            var startIndex = i;
            tile.DoubleTapped += (_, _) => OpenLocalArtworkLightbox(localArtwork, startIndex);
            var path = art.Path;
            _ = Task.Run(() => TryLoadFileBitmap(path)).ContinueWith(
                task => { if (task.Result is { } bitmap) { image.Source = bitmap; } },
                TaskScheduler.FromCurrentSynchronizationContext());
        }

        section.Children.Add(grid);
        return section;
    }

    private void OpenLocalArtworkLightbox(List<LocalArtwork> localArtwork, int startIndex)
    {
        var entries = new List<LightboxEntry>();
        foreach (var art in localArtwork)
        {
            var path = art.Path;
            entries.Add(new LightboxEntry(art.FileId, System.IO.Path.GetFileName(art.Path), () =>
            {
                try
                {
                    return System.IO.File.ReadAllBytes(path);
                }
                catch (Exception)
                {
                    return null;
                }
            }));
        }
        _lightbox.Show(entries, startIndex);
    }

    private static Bitmap? TryLoadFileBitmap(string path)
    {
        try
        {
            return new Bitmap(path);
        }
        catch (Exception)
        {
            return null;
        }
    }

    // The document viewer: the file's decoded text, monospace and selectable, in a
    // scrollable modal — the modal-host shape of the WinUI document dialog.
    private Task ShowDocument(string name, string text) => _host.Show(close =>
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(name));
        column.Children.Add(new ScrollViewer
        {
            MaxHeight = 480,
            Content = new SelectableTextBlock
            {
                Text = text,
                FontFamily = new FontFamily("monospace"),
                TextWrapping = TextWrapping.Wrap,
            },
        });
        var ok = DialogUi.Primary(Loc.Chrome("action.close"));
        ok.Click += (_, _) => close();
        column.Children.Add(DialogUi.Actions(ok));
        return column;
    });

    // A dismiss-only message dialog: a title over an optional body, closed by OK.
    private Task ShowMessage(string title, string? body) => _host.Show(close =>
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(title));
        if (body is not null)
        {
            column.Children.Add(DialogUi.Body(body));
        }
        var ok = DialogUi.Primary(Loc.Chrome("action.ok"));
        ok.Click += (_, _) => close();
        column.Children.Add(DialogUi.Actions(ok));
        return column;
    });
}
