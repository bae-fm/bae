using System;
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

    public ReleaseActionDialogs(AppService app, ModalHost host)
    {
        _app = app;
        _host = host;
    }

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
