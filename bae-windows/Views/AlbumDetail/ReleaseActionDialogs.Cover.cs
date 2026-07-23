using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The cover-art dialogs: the gallery view and the change-cover picker. Split
// out of ReleaseActionDialogs.cs unchanged.
internal sealed partial class ReleaseActionDialogs
{
    public System.Threading.Tasks.Task ShowGallery(string releaseId)
    {
        var (current, images) = _session.WithCurrentHandle(
            handle => NativeBae.Gallery(handle, releaseId).Items);
        if (!current)
        {
            return System.Threading.Tasks.Task.CompletedTask;
        }
        if (images is null)
        {
            _setStatus(Loc.Chrome("gallery.load_failed"));
            return System.Threading.Tasks.Task.CompletedTask;
        }

        if (images.Length == 0)
        {
            return System.Threading.Tasks.Task.CompletedTask;
        }

        var entries = new List<LightboxEntry>();
        foreach (var item in images)
        {
            var capturedReleaseId = releaseId;
            var capturedSource = item.Source;
            var capturedLabel = item.Label;

            entries.Add(new LightboxEntry(
                item.Id,
                capturedLabel,
                () =>
                {
                    var handle = _session.CurrentHandleOrNull();
                    if (handle is null)
                    {
                        return null;
                    }
                    return handle.TryUse(
                        app => NativeBae.GalleryBytes(app, capturedReleaseId, capturedSource),
                        out var bytes)
                        ? bytes
                        : null;
                }));
        }

        _lightbox.Show(entries, 0);
        return System.Threading.Tasks.Task.CompletedTask;
    }

    // Pick a new cover for the release: its own image files plus remote candidates
    // fetched from MusicBrainz / Discogs. Selecting one writes it as the release's
    // cover; the album grid refreshes via the invalidation the change emits. Errors
    // surface inside this dialog, since the window banner is occluded by the modal.
    public async System.Threading.Tasks.Task ShowChangeCover(string albumId, string releaseId)
    {
        if (string.IsNullOrEmpty(albumId) || string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        var (imagesCurrent, releaseImages) = _session.WithCurrentHandle(
            handle => NativeBae.GetReleaseImages(handle, releaseId).Images);
        if (!imagesCurrent)
        {
            return;
        }
        if (releaseImages is null)
        {
            _setStatus(Loc.Chrome("cover.images_load_failed"));
            return;
        }

        var content = new StackPanel { Spacing = 8, MinWidth = 460 };

        // Errors from a failed remote fetch or a failed change surface here; the
        // window-level banner is hidden behind this modal dialog.
        var statusText = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(statusText);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("cover.change_title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 520 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };

        // Apply a selection off the UI thread (a remote cover downloads bytes),
        // then close on success or show the error in place.
        async System.Threading.Tasks.Task Apply(BridgeCoverSelection selection)
        {
            statusText.Visibility = Visibility.Collapsed;
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ChangeCover(handle, albumId, releaseId, selection));
            if (!current)
            {
                return;
            }
            if (error is null)
            {
                dialog.Hide();
            }
            else
            {
                statusText.Text = error;
                statusText.Visibility = Visibility.Visible;
            }
        }

        // A thumbnail tile that applies the selection when clicked.
        Button Tile(ImageSource? source, string caption, BridgeCoverSelection selection)
        {
            var button = DialogPrimitives.CoverTile(source, caption);
            button.Click += async (_, _) => await Apply(selection);
            return button;
        }

        if (releaseImages.Length > 0)
        {
            content.Children.Add(new TextBlock { Text = Loc.Chrome("cover.release_files") });
            var fileGrid = new VariableSizedWrapGrid
            {
                Orientation = Orientation.Horizontal,
                ItemWidth = 140,
                ItemHeight = 160,
            };
            foreach (var file in releaseImages)
            {
                var handle = _session.CurrentHandleOrNull();
                if (handle == null)
                {
                    return;
                }
                var source = CoverImage.LoadGalleryBytes(
                    handle, releaseId, new BridgeGallerySource.ReleaseFile(file.Id));
                var selection = new BridgeCoverSelection.ReleaseImage(file.Id);
                fileGrid.Children.Add(Tile(source, file.OriginalFilename, selection));
            }

            content.Children.Add(fileGrid);
        }

        content.Children.Add(new TextBlock { Text = Loc.Chrome("cover.remote_sources") });
        var loading = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        loading.Children.Add(new ProgressRing { IsActive = true, Width = 20, Height = 20 });
        loading.Children.Add(new TextBlock { Text = Loc.Chrome("cover.fetching") });
        content.Children.Add(loading);

        var remoteGrid = new VariableSizedWrapGrid
        {
            Orientation = Orientation.Horizontal,
            ItemWidth = 140,
            ItemHeight = 160,
        };
        content.Children.Add(remoteGrid);

        // Fetch the remote candidates off the UI thread, then fill the grid on
        // resume. The dialog opens immediately with the release files shown and a
        // spinner where the remote covers will land.
        async System.Threading.Tasks.Task LoadRemote()
        {
            var (current, covers) = await _session.RunForCurrentHandle(
                handle => NativeBae.FetchRemoteCovers(handle, releaseId).Covers);
            if (!current)
            {
                return;
            }
            loading.Visibility = Visibility.Collapsed;
            if (covers is null)
            {
                statusText.Text = Loc.Chrome("cover.fetch_failed");
                statusText.Visibility = Visibility.Visible;
                return;
            }

            try
            {
                if (covers.Length == 0)
                {
                    remoteGrid.Children.Add(new TextBlock { Text = Loc.Chrome("cover.none_remote") });
                    return;
                }

                foreach (var cover in covers)
                {
                    var source = new BitmapImage(new Uri(NativeBae.RemoteCoverThumbnailUrl(cover)));
                    var selection = NativeBae.RemoteCoverSelection(cover);
                    remoteGrid.Children.Add(Tile(source, cover.Label, selection));
                }
            }
            catch (Exception ex)
            {
                // Fire-and-forget: a malformed cover URL or unexpected payload must
                // surface here, not as an unobserved task exception.
                statusText.Text = Loc.Chrome("cover.show_failed", "detail", ex.Message);
                statusText.Visibility = Visibility.Visible;
            }
        }

        _ = LoadRemote();
        await dialog.ShowAsync();
    }
}
