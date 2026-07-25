using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
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
