using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// What the import needs a modal for, now that the mapping pane holds the flow
// itself: reading one of the folder's documents, choosing a cover, jumping to
// an album, and saying something the pane has no line for. The picker and the
// confirmation sheet are gone — identifying, mapping and committing all happen
// in the pane, in place, with nothing sliding over anything else.
//
// One presenter per main window; every read/write runs through a domain
// service, so no view here touches NativeBae.
internal sealed class ImportDialogs
{
    private readonly ModalHost _host;
    private readonly LightboxOverlay _lightbox;
    private readonly ImageStore _images;
    private readonly Func<string, Task> _openAlbum;

    public ImportDialogs(
        ModalHost host,
        LightboxOverlay lightbox,
        ImageStore images,
        Func<string, Task> openAlbum)
    {
        _host = host;
        _lightbox = lightbox;
        _images = images;
        _openAlbum = openAlbum;
    }

    /// <summary>Open an album in the library — the already-in-library banner's
    /// jump to the duplicate.</summary>
    internal Task OpenAlbum(string albumId) => _openAlbum(albumId);

    /// <summary>Show one of the folder's readable evidence files (a track
    /// sheet, a rip log, an info text) decoded to its text, or say why it could
    /// not be read.</summary>
    internal Task ShowDocumentFile(ImportDocument document)
    {
        var (text, error) = ImportService.ReadDocumentText(document.Path);
        if (error is not null || text is null)
        {
            return ShowMessage(
                Loc.Chrome("import.error_title"),
                Loc.Chrome(
                    "import.document.read_failed",
                    new Dictionary<string, object?> { ["name"] = document.Name, ["error"] = error ?? string.Empty }));
        }
        return ShowDocument(document.Name, text);
    }

    /// <summary>
    /// The cover choices for the release being imported: the source release's
    /// remote covers (thumbnails by URL) and the candidate folder's own images
    /// (thumbnails by disk path). Clicking a tile picks it; double-tapping a
    /// local image opens the lightbox. Closing without a pick leaves the import
    /// to choose its own default.
    /// </summary>
    internal Task ShowCoverPicker(
        List<BridgeRemoteCover> remoteCovers,
        List<LocalArtwork> localArtwork,
        Action<PickedCover> onPick) =>
        _host.Show(close =>
        {
            var column = DialogUi.Column();
            column.MinWidth = 460;
            column.Children.Add(DialogUi.Title(Loc.Chrome("cover.change_title")));
            column.Children.Add(BuildCoverPicker(remoteCovers, localArtwork, picked =>
            {
                onPick(picked);
                close();
            }));
            var cancel = new Button { Content = Loc.Chrome("action.cancel") };
            cancel.Click += (_, _) => close();
            column.Children.Add(DialogUi.Actions(cancel));
            return new ScrollViewer { Content = column, MaxHeight = 560 };
        });

    /// <summary>Confirm replacing the candidate's metadata draft with its
    /// blank shape. Files and mapping decisions are unaffected.</summary>
    internal Task ConfirmClearMetadata(Func<Task> clear) => _host.Show(close =>
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("import.metadata.clear_title")));
        column.Children.Add(DialogUi.Body(Loc.Chrome("import.metadata.clear_body")));

        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        cancel.Click += (_, _) => close();
        var confirm = DialogUi.Primary(Loc.Chrome("import.metadata.clear"));
        confirm.Click += async (_, _) =>
        {
            confirm.IsEnabled = false;
            await clear();
            close();
        };
        column.Children.Add(DialogUi.Actions(cancel, confirm));
        return column;
    });

    private StackPanel BuildCoverPicker(
        List<BridgeRemoteCover> remoteCovers, List<LocalArtwork> localArtwork, Action<PickedCover> onPick)
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
        void Select(Button picked, PickedCover cover)
        {
            foreach (var tile in tiles)
            {
                tile.BorderThickness = new Thickness(0);
            }
            picked.BorderThickness = new Thickness(2);
            picked.BorderBrush = Brushes.DeepSkyBlue;
            onPick(cover);
        }

        Button AddTile(Image image, string caption, PickedCover cover)
        {
            var tile = DialogUi.CoverTile(image, caption);
            tile.Click += (_, _) => Select(tile, cover);
            tiles.Add(tile);
            grid.Children.Add(tile);
            return tile;
        }

        foreach (var cover in remoteCovers)
        {
            var image = new Image();
            var url = ReleaseEditorService.RemoteCoverThumbnailUrl(cover);
            AddTile(image, cover.Label, new PickedCover(ReleaseEditorService.RemoteCoverSelection(cover), false, url));
            _images.Bind(image, new ImageContent.Remote(url), ImageWidths.PickerTile);
        }

        for (var i = 0; i < localArtwork.Count; i++)
        {
            var art = localArtwork[i];
            var image = new Image();
            var tile = AddTile(
                image,
                System.IO.Path.GetFileName(art.FileId),
                new PickedCover(new BridgeCoverSelection.ReleaseImage(art.FileId), true, art.Path));
            var startIndex = i;
            tile.DoubleTapped += (_, _) => OpenLocalArtworkLightbox(localArtwork, startIndex);
            _images.Bind(image, new ImageContent.LocalFile(art.Path), ImageWidths.PickerTile);
        }

        section.Children.Add(grid);
        return section;
    }

    /// <summary>Open the folder's images in the lightbox, at the one whose path
    /// was clicked. An image the folder no longer holds has nothing to show, so
    /// there is nothing to open.</summary>
    internal void ShowFolderImages(List<LocalArtwork> images, string path)
    {
        var startIndex = images.FindIndex(art => art.Path == path);
        if (startIndex < 0)
        {
            BaeDiagnostics.Logger.Warning($"no image at {path} among the folder's {images.Count}");
            return;
        }
        OpenLocalArtworkLightbox(images, startIndex);
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

    /// <summary>The content for whichever face a picked cover has — one of the
    /// folder's own images off disk, or a remote thumbnail by URL.</summary>
    internal static ImageContent CoverFaceContent(bool isLocal, string source) =>
        isLocal ? new ImageContent.LocalFile(source) : new ImageContent.Remote(source);

    /// <summary>Where a cover's thumbnail is read from. Core says which of the
    /// two it is, so no surface decides from a URL's shape.</summary>
    internal static ImageContent CoverChoiceContent(BridgeCoverChoice cover) =>
        cover.ThumbnailSource switch
        {
            BridgeCoverImageSource.Local local => new ImageContent.LocalFile(local.Path),
            BridgeCoverImageSource.Remote remote => new ImageContent.Remote(remote.Url),
            BridgeCoverImageSource.Bytes bytes => new ImageContent.Bytes(bytes.Data),
            _ => throw new ArgumentOutOfRangeException(
                nameof(cover), cover.ThumbnailSource, "Unknown cover image source"),
        };


    // The document viewer: the file's decoded text, monospace and selectable, in a
    // scrollable modal.
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

/// <summary>
/// A cover the user picked, with the face to show for it. The selection is what
/// the commit takes; <see cref="Source"/> is a remote thumbnail URL or a path on
/// disk, per <see cref="IsLocal"/>, because a remote selection carries a payload
/// the UI cannot read a thumbnail back out of.
/// </summary>
internal sealed record PickedCover(BridgeCoverSelection Selection, bool IsLocal, string Source);
