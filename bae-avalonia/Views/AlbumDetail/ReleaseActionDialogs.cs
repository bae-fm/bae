using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The per-release actions reachable from album detail, presented in the window's
// modal host (the in-window equivalent of the macOS sheets). One presenter per
// main window, threaded to the inline album expansion; each method opens its
// dialog through the host and runs its writes through the ReleaseEditor service,
// so no view here touches NativeBae. The dialog family grows with the parity
// port — change cover here, then the gallery lightbox, edit metadata, and
// re-identify.
internal sealed class ReleaseActionDialogs
{
    private readonly AppService _app;
    private readonly ModalHost _host;
    private readonly LightboxOverlay _lightbox;

    public ReleaseActionDialogs(AppService app, ModalHost host, LightboxOverlay lightbox)
    {
        _app = app;
        _host = host;
        _lightbox = lightbox;
    }

    // Open the release's gallery in the lightbox. The items come from the loaded
    // release detail (as on macOS); each entry reads its bytes on demand through
    // the image store, which fetches and decrypts from the cloud home when
    // off-disk. The lightbox decodes at native resolution itself, so these are raw
    // bytes rather than one of the store's sized decodes.
    public void ShowGallery(string releaseId, IReadOnlyList<BridgeGalleryItem> items)
    {
        var entries = new List<LightboxEntry>();
        foreach (var item in items)
        {
            var source = item.Source;
            entries.Add(new LightboxEntry(
                item.Id,
                item.Label,
                () => _app.Images.ReadBytes(new ImageContent.ReleaseImage(releaseId, source))));
        }
        _lightbox.Show(entries, 0);
    }

    // Edit the release's metadata — album / pressing fields and the per-track table.
    // The seed is read before the dialog opens; Save commits (shaping and validation
    // happen in core — a validation error keeps the dialog open with the reason),
    // and Reset re-seeds from the release's stored source without writing.
    public async Task ShowEditMetadata(string releaseId)
    {
        var (current, result) = await _app.ReleaseEditor.ReleaseEditSeed(releaseId);
        if (!current)
        {
            return;
        }
        if (result.Edit is not { } seed)
        {
            await ShowMessage(Loc.Chrome("album.edit.title"), Loc.Chrome("album.edit.load_failed"));
            return;
        }
        await _host.Show(close => BuildEditMetadata(releaseId, seed, close));
    }

    private Control BuildEditMetadata(string releaseId, BridgeRawReleaseEdit seed, Action close)
    {
        var form = new ReleaseEditForm(seed, 460);
        var column = DialogUi.Column();
        column.MinWidth = 460;
        column.Children.Add(DialogUi.Title(Loc.Chrome("album.edit.title")));
        column.Children.Add(new ScrollViewer { Content = form.Panel, MaxHeight = 460 });

        var save = DialogUi.Primary(Loc.Chrome("action.save"));
        save.Click += async (_, _) =>
        {
            var (current, error) = await _app.ReleaseEditor.ApplyReleaseEdit(releaseId, form.ReadBack());
            if (!current)
            {
                return;
            }
            if (error is null)
            {
                close();
            }
            else
            {
                form.ErrorText.Text = error;
                form.ErrorText.IsVisible = true;
            }
        };
        var reset = new Button { Content = Loc.Chrome("album.edit.reset") };
        reset.Click += async (_, _) =>
        {
            var (current, result) = await _app.ReleaseEditor.ResetMetadataToSource(releaseId);
            if (!current)
            {
                return;
            }
            if (result.Edit is { } fresh)
            {
                form.ErrorText.IsVisible = false;
                form.Seed(fresh);
            }
            else
            {
                form.ErrorText.Text = Loc.Chrome("album.edit.reset_failed");
                form.ErrorText.IsVisible = true;
            }
        };
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        cancel.Click += (_, _) => close();
        column.Children.Add(DialogUi.Actions(cancel, reset, save));
        return column;
    }

    // Re-identify the release: an auto-identify pipeline (its live status, signal
    // badges, and match list) with a manual search fallback. Committing a
    // source-backed choice offers to reseed the metadata from the newly-pointed
    // source.
    public async Task ShowReidentify(string releaseId, string seedArtist, string seedAlbum)
    {
        var key = "reidentify:" + releaseId;
        var confirmed = false;
        Action<BridgeCandidateRuntimeChange>? onRuntimeChanged = null;

        await _host.Show(close =>
        {
            var pipelineStatus = DialogUi.Body(Loc.Chrome("identify.identifying"));
            var badgeHost = new StackPanel();
            var resultsList = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 260 };
            var status = DialogUi.Danger();

            var artistField = DialogUi.Field(Loc.Chrome("search.field.artist"), out var artistBox);
            var albumField = DialogUi.Field(Loc.Chrome("search.field.album"), out var albumBox);
            artistBox.Text = seedArtist;
            albumBox.Text = seedAlbum;
            var sourceBox = new ComboBox { ItemsSource = new[] { "discogs", "musicbrainz" }, SelectedIndex = 0, HorizontalAlignment = HorizontalAlignment.Stretch };
            var sourceCaption = new TextBlock { Text = Loc.Chrome("search.field.source"), FontSize = 12.5 };
            sourceCaption[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            var sourceField = new StackPanel { Spacing = 4, Children = { sourceCaption, sourceBox } };
            var searchButton = new Button { Content = Loc.Chrome("action.search") };

            // The pressing the user picked, pending the confirm that commits
            // it — re-identify writes the same IdentityChoice an import does.
            var pickedIndex = -1;

            var candidates = new List<ReleaseCandidateChoice>();
            var results = new ReidentifyResultsModel();

            var confirm = DialogUi.Primary(Loc.Chrome("album.reidentify.confirm"));
            confirm.IsEnabled = false;

            void ToggleSignal(string kind, string value)
            {
                results.ResumePipeline();
                _ = _app.Import.ToggleSignalForCandidate(key, kind, value);
            }

            void Rerun()
            {
                results.ResumePipeline();
                _ = _app.Import.RerunIdentifyForCandidate(key);
            }

            // The re-identify run has no candidate row anywhere — the release
            // is already in the library — so its state comes from the
            // candidate-runtime signal under this dialog's key.
            void ShowRun(BridgeCandidateRuntimeSnapshot? runtime)
            {
                var (status, matches, badges) = _app.Import.ProjectRun(runtime);
                var line = status?.LocalizedLine ?? string.Empty;
                pipelineStatus.Text = line;
                pipelineStatus.IsVisible = line.Length > 0;
                badgeHost.Children.Clear();
                if (badges.Count > 0)
                {
                    badgeHost.Children.Add(SignalBadgeRow.Build(badges, ToggleSignal, Rerun));
                }
                if (results.ApplyPipelineMatches(matches.Select(match => match.ReleaseId).ToList()))
                {
                    candidates = matches;
                    resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
                }
            }

            void OnRuntimeChanged(BridgeCandidateRuntimeChange change)
            {
                switch (change)
                {
                    case BridgeCandidateRuntimeChange.Updated updated when updated.Key == key:
                        ShowRun(updated.Runtime);
                        break;
                    case BridgeCandidateRuntimeChange.Removed removed when removed.Key == key:
                        ShowRun(null);
                        break;
                    case BridgeCandidateRuntimeChange.Reset reset:
                        ShowRun(reset.Runtimes
                            .FirstOrDefault(entry => entry.Key == key)?.Runtime);
                        break;
                }
            }
            onRuntimeChanged = OnRuntimeChanged;

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
                results.ApplyManualResults();
                pickedIndex = -1;
                candidates = search.Candidates ?? new List<ReleaseCandidateChoice>();
                resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
                status.Text = Loc.Chrome("search.no_matches");
                status.IsVisible = candidates.Count == 0;
                confirm.IsEnabled = false;
            };

            // Picking a row claims that pressing.
            resultsList.SelectionChanged += (_, _) =>
            {
                pickedIndex = resultsList.SelectedIndex;
                confirm.IsEnabled = pickedIndex >= 0 && pickedIndex < candidates.Count;
            };

            confirm.Click += async (_, _) =>
            {
                if (pickedIndex < 0 || pickedIndex >= candidates.Count)
                {
                    return;
                }
                var picked = candidates[pickedIndex];
                var (current, error) = await _app.ReleaseEditor.ReidentifyRelease(
                    releaseId, new BridgeIdentityChoice.Release(picked.ReleaseId, picked.Source));
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.IsVisible = true;
                    return;
                }
                confirmed = true;
                close();
            };

            var skip = new Button { Content = Loc.Chrome("identify.skip") };
            skip.Click += async (_, _) =>
            {
                var (current, error) = await _app.ReleaseEditor.ReidentifyRelease(releaseId, new BridgeIdentityChoice.Unknown());
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.IsVisible = true;
                    return;
                }
                close();
            };
            var cancel = new Button { Content = Loc.Chrome("action.cancel") };
            cancel.Click += (_, _) => close();

            var column = DialogUi.Column();
            column.MinWidth = 440;
            column.Children.Add(DialogUi.Title(Loc.Chrome("album.reidentify.title")));
            column.Children.Add(pipelineStatus);
            column.Children.Add(badgeHost);
            column.Children.Add(resultsList);
            column.Children.Add(artistField);
            column.Children.Add(albumField);
            column.Children.Add(sourceField);
            column.Children.Add(searchButton);
            column.Children.Add(status);
            column.Children.Add(DialogUi.Actions(cancel, skip, confirm));

            // Start the pipeline against the release's own files; candidate values
            // update the import store while it runs.
            // Subscribe before starting the run, so nothing it reports is
            // missed between the two.
            _app.ImportStore.CandidateRuntimeChanged += OnRuntimeChanged;
            _app.Import.AutoIdentifyRelease(key, releaseId);
            return new ScrollViewer { Content = column, MaxHeight = 560 };
        });

        if (onRuntimeChanged is not null)
        {
            _app.ImportStore.CandidateRuntimeChanged -= onRuntimeChanged;
        }
        _app.Import.CancelAutoIdentify(key);
        if (confirmed)
        {
            await ShowRefreshPrompt(releaseId);
        }
    }

    // After a source-backed identity commit, offer to reseed the metadata from the
    // newly-pointed source (overwriting prior edits by design); Keep leaves it.
    private Task ShowRefreshPrompt(string releaseId) => _host.Show(close =>
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("album.reidentify.updated")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("album.reidentify.refresh_body")));
        var error = DialogUi.Danger();
        column.Children.Add(error);
        var refresh = DialogUi.Primary(Loc.Chrome("album.reidentify.refresh_confirm"));
        refresh.Click += async (_, _) =>
        {
            var (current, refreshError) = await _app.ReleaseEditor.RefreshMetadataFromSource(releaseId);
            if (!current)
            {
                return;
            }
            if (refreshError is not null)
            {
                error.Text = refreshError;
                error.IsVisible = true;
                return;
            }
            close();
        };
        var keep = new Button { Content = Loc.Chrome("album.reidentify.keep_current") };
        keep.Click += (_, _) => close();
        column.Children.Add(DialogUi.Actions(keep, refresh));
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

    // Pick a new cover for the release — the release's own image files plus remote
    // candidates fetched from MusicBrainz / Discogs. Selecting one writes it; the
    // open album subscription delivers the new cover. Errors surface inside
    // the dialog, since the window banner is occluded by the modal. Remote sources
    // lead (with a Refresh), then the release files, matching the macOS sheet.
    public Task ShowChangeCover(string releaseId, IReadOnlyList<BridgeFile> releaseImages) =>
        _host.Show(close => BuildChangeCover(releaseId, releaseImages, close));

    private Control BuildChangeCover(
        string releaseId,
        IReadOnlyList<BridgeFile> releaseImages,
        Action close)
    {
        var column = DialogUi.Column();
        column.MinWidth = 460;
        column.Children.Add(DialogUi.Title(Loc.Chrome("cover.change_title")));

        var error = DialogUi.Danger();
        column.Children.Add(error);

        void ShowError(string message)
        {
            error.Text = message;
            error.IsVisible = true;
        }

        async Task Apply(BridgeCoverSelection selection)
        {
            error.IsVisible = false;
            var (current, changeError) = await _app.ReleaseEditor.ChangeCover(releaseId, selection);
            if (!current)
            {
                return;
            }
            if (changeError is null)
            {
                close();
            }
            else
            {
                ShowError(changeError);
            }
        }

        Button Tile(Image image, string caption, BridgeCoverSelection selection)
        {
            var tile = DialogUi.CoverTile(image, caption);
            tile.Click += async (_, _) => await Apply(selection);
            return tile;
        }

        // ── Remote sources ──────────────────────────────────────────────────────
        column.Children.Add(DialogUi.SectionLabel(Loc.Chrome("cover.remote_sources")));
        var loading = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        loading.Children.Add(new Spinner { Width = 18, Height = 18 });
        loading.Children.Add(DialogUi.Body(Loc.Chrome("cover.fetching")));
        column.Children.Add(loading);
        var remoteGrid = new WrapPanel { Orientation = Orientation.Horizontal };
        column.Children.Add(remoteGrid);
        var refresh = new Button { Content = Loc.Chrome("cover.refresh") };
        column.Children.Add(refresh);

        async Task LoadRemote()
        {
            loading.IsVisible = true;
            remoteGrid.Children.Clear();
            refresh.IsEnabled = false;
            var (current, result) = await _app.ReleaseEditor.FetchRemoteCovers(releaseId);
            refresh.IsEnabled = true;
            if (!current)
            {
                return;
            }
            loading.IsVisible = false;
            if (result.Covers is null)
            {
                ShowError(Loc.Chrome("cover.fetch_failed"));
                return;
            }
            if (result.Covers.Length == 0)
            {
                remoteGrid.Children.Add(DialogUi.Body(Loc.Chrome("cover.none_remote")));
                return;
            }
            foreach (var cover in result.Covers)
            {
                var image = new Image();
                var url = ReleaseEditorService.RemoteCoverThumbnailUrl(cover);
                remoteGrid.Children.Add(Tile(image, cover.Label, ReleaseEditorService.RemoteCoverSelection(cover)));
                _app.Images.Bind(image, new ImageContent.Remote(url), ImageWidths.PickerTile);
            }
        }

        refresh.Click += async (_, _) => await LoadRemote();
        _ = LoadRemote();

        // ── Release files ───────────────────────────────────────────────────────
        if (releaseImages.Count > 0)
        {
            column.Children.Add(DialogUi.SectionLabel(Loc.Chrome("cover.release_files")));
            var fileGrid = new WrapPanel { Orientation = Orientation.Horizontal };
            foreach (var file in releaseImages)
            {
                var image = new Image();
                _app.Images.Bind(
                    image,
                    new ImageContent.ReleaseImage(
                        releaseId, new BridgeGallerySource.ReleaseFile(file.Id)),
                    ImageWidths.PickerTile);
                fileGrid.Children.Add(Tile(image, file.OriginalFilename, new BridgeCoverSelection.ReleaseImage(file.Id)));
            }
            column.Children.Add(fileGrid);
        }

        var done = new Button { Content = Loc.Chrome("action.done") };
        done.Click += (_, _) => close();
        column.Children.Add(DialogUi.Actions(done));

        return new ScrollViewer { Content = column, MaxHeight = 520 };
    }
}
