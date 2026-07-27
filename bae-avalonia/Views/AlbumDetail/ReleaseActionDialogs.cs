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
// modal host (the cross-platform stand-in for WinUI's ContentDialog and the macOS
// sheets). One presenter per main window, threaded to the inline album expansion;
// each method opens its dialog through the host and runs its writes through the
// ReleaseEditor service, so no view here touches NativeBae. The dialog family grows
// with the parity port — change cover here, then the gallery lightbox, edit
// metadata, and re-identify.
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
    // MediaPaths, which fetches and decrypts from the cloud home when off-disk.
    public void ShowGallery(string releaseId, IReadOnlyList<BridgeGalleryItem> items)
    {
        var entries = new List<LightboxEntry>();
        foreach (var item in items)
        {
            var source = item.Source;
            entries.Add(new LightboxEntry(item.Id, item.Label, () => _app.MediaPaths.FetchGalleryBytes(releaseId, source)));
        }
        _lightbox.Show(entries, 0);
    }

    // Edit the release's metadata — album / pressing fields and the per-track table.
    // The seed is read before the dialog opens; Save commits (shaping and validation
    // happen in core — a validation error keeps the dialog open with the reason),
    // and Reset re-seeds from the release's stored source without writing.
    public Task ShowEditMetadata(string releaseId)
    {
        var (current, result) = _app.ReleaseEditor.ReleaseEditSeed(releaseId);
        if (!current)
        {
            return Task.CompletedTask;
        }
        if (result.Edit is not { } seed)
        {
            return ShowMessage(Loc.Chrome("album.edit.title"), Loc.Chrome("album.edit.load_failed"));
        }
        return _host.Show(close => BuildEditMetadata(releaseId, seed, close));
    }

    private Control BuildEditMetadata(string releaseId, BridgeRawReleaseEdit seed, Action close)
    {
        var form = new ReleaseEditForm(seed, 460);
        var column = DialogUi.Column();
        column.MinWidth = 460;
        column.Children.Add(DialogUi.Title(Loc.Chrome("album.edit.title")));
        column.Children.Add(new ScrollViewer { Content = form.Panel, MaxHeight = 460 });

        var save = DialogUi.Primary(Loc.Chrome("action.save"));
        save.Click += (_, _) =>
        {
            var (current, error) = _app.ReleaseEditor.ApplyReleaseEdit(releaseId, form.ReadBack());
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
    // badges, and match list) with a manual search fallback and an exact / metadata
    // identity claim. Committing a source-backed choice offers to reseed the
    // metadata from the newly-pointed source. Mirrors the WinUI flow, over the
    // service layer and the modal host.
    public async Task ShowReidentify(string releaseId, string seedArtist, string seedAlbum)
    {
        var key = "reidentify:" + releaseId;
        IDisposable? registration = null;
        var confirmed = false;

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

            // What the picked pressing claims, stated the way the import
            // confirm pane states it — re-identify writes the same
            // IdentityChoice. Hidden until a result is picked; re-derived on
            // each pick, which is the only thing that moves it.
            BridgeClaimLine? claim = null;
            var claimRow = new StackPanel { IsVisible = false };

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

            void RefreshFromPipeline()
            {
                var (current, snapshot) = _app.Import.ReidentifyPipeline(key);
                if (!current || snapshot is not { } snap)
                {
                    return;
                }
                pipelineStatus.Text = snap.RowStatus.LocalizedLine;
                pipelineStatus.IsVisible = !string.IsNullOrEmpty(snap.RowStatus.LocalizedLine);
                badgeHost.Children.Clear();
                if (snap.Signals.Count > 0)
                {
                    badgeHost.Children.Add(SignalBadgeRow.Build(snap.Signals, ToggleSignal, Rerun));
                }
                if (results.ApplyPipelineMatches(snap.Matches.Select(match => match.ReleaseId).ToList()))
                {
                    candidates = snap.Matches;
                    resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
                }
            }

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
                candidates = search.Candidates ?? new List<ReleaseCandidateChoice>();
                resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
                status.Text = Loc.Chrome("search.no_matches");
                status.IsVisible = candidates.Count == 0;
                confirm.IsEnabled = false;
            };

            resultsList.SelectionChanged += (_, _) =>
            {
                var index = resultsList.SelectedIndex;
                claim = index >= 0 && index < candidates.Count
                    ? _app.Import.ClaimForPick(key, candidates[index].Pressing).Claim
                    : null;
                confirm.IsEnabled = claim is not null;
                claimRow.Children.Clear();
                if (claim is { } picked)
                {
                    claimRow.Children.Add(ClaimLineView.Build(picked));
                }
                claimRow.IsVisible = claim is not null;
            };

            confirm.Click += async (_, _) =>
            {
                if (claim is not { } picked)
                {
                    return;
                }
                var (current, error) = await _app.ReleaseEditor.ReidentifyRelease(releaseId, picked.Choice);
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
            column.Children.Add(claimRow);
            column.Children.Add(artistField);
            column.Children.Add(albumField);
            column.Children.Add(sourceField);
            column.Children.Add(searchButton);
            column.Children.Add(status);
            column.Children.Add(DialogUi.Actions(cancel, skip, confirm));

            // Start the pipeline against the release's own files; its events flow
            // through the candidate invalidation the registration reads.
            _app.Import.AutoIdentifyRelease(key, releaseId);
            registration = _app.ProjectionRegistry.Register(typeof(BridgeInvalidation.ImportCandidate), RefreshFromPipeline);
            return new ScrollViewer { Content = column, MaxHeight = 560 };
        });

        registration?.Dispose();
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
    // grid refreshes via the invalidation the change emits. Errors surface inside
    // the dialog, since the window banner is occluded by the modal. Remote sources
    // lead (with a Refresh), then the release files, matching the macOS sheet.
    public Task ShowChangeCover(string albumId, string releaseId) =>
        _host.Show(close => BuildChangeCover(albumId, releaseId, close));

    private Control BuildChangeCover(string albumId, string releaseId, Action close)
    {
        var (imagesCurrent, imagesResult) = _app.ReleaseEditor.GetReleaseImages(releaseId);
        var releaseImages = imagesCurrent ? imagesResult.Images : null;

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
            var (current, changeError) = await _app.ReleaseEditor.ChangeCover(albumId, releaseId, selection);
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
                _ = CoverImage.LoadUrlAsync(url).ContinueWith(
                    task =>
                    {
                        if (task.Result is { } bitmap)
                        {
                            image.Source = bitmap;
                        }
                    },
                    TaskScheduler.FromCurrentSynchronizationContext());
            }
        }

        refresh.Click += async (_, _) => await LoadRemote();
        _ = LoadRemote();

        // ── Release files ───────────────────────────────────────────────────────
        if (releaseImages is { Length: > 0 })
        {
            column.Children.Add(DialogUi.SectionLabel(Loc.Chrome("cover.release_files")));
            var fileGrid = new WrapPanel { Orientation = Orientation.Horizontal };
            foreach (var file in releaseImages)
            {
                var image = new Image
                {
                    Source = CoverImage.LoadGalleryBytes(
                        _app.MediaPaths, releaseId, new BridgeGallerySource.ReleaseFile(file.Id)),
                };
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
