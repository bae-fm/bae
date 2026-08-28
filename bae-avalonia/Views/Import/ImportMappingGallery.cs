using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The folder's images as a full-width gallery of fixed thumbnails. A picture
/// is read by looking at it, and clicking one opens the ordered gallery at that
/// image.
/// </summary>
internal sealed class ImportMappingGallery
{
    internal const double TileSize = 96;
    internal static readonly DataFormat<string> CoverDragFormat =
        DataFormat.CreateStringApplicationFormat(
            "application/x-bae-import-cover-file-id");

    private readonly IReadOnlyList<BridgeMappingImage> _images;
    private readonly IReadOnlyList<BridgeFileEvidence> _evidence;
    private readonly Action<Image, string> _loadImage;
    private readonly Action<IReadOnlyList<BridgeMappingImage>, string> _openImages;

    internal ImportMappingGallery(
        IReadOnlyList<BridgeMappingImage> images,
        IReadOnlyList<BridgeFileEvidence> evidence,
        Action<Image, string> loadImage,
        Action<IReadOnlyList<BridgeMappingImage>, string> openImages)
    {
        _images = images;
        _evidence = evidence;
        _loadImage = loadImage;
        _openImages = openImages;
    }

    internal Control Build()
    {
        var gallery = new WrapPanel
        {
            Orientation = Orientation.Horizontal,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        foreach (var image in _images)
        {
            gallery.Children.Add(Tile(image));
        }
        return gallery;
    }

    private Button Tile(BridgeMappingImage image)
    {
        var thumbnail = new Image
        {
            Width = TileSize,
            Height = TileSize,
            Stretch = Stretch.UniformToFill,
        };
        // The mark sits over the picture, so the frame holds a stack rather
        // than the image alone.
        var stacked = new Panel();
        stacked.Children.Add(thumbnail);
        var found = ImportEvidence.Of(image.FileId, _evidence);
        if (found is not null)
        {
            var mark = ImportEvidence.Chip(found, onImage: true);
            mark.HorizontalAlignment = HorizontalAlignment.Left;
            mark.VerticalAlignment = VerticalAlignment.Bottom;
            mark.Margin = new Thickness(3);
            stacked.Children.Add(mark);
        }
        var frame = new Border
        {
            Width = TileSize,
            Height = TileSize,
            CornerRadius = new CornerRadius(4),
            ClipToBounds = true,
            Child = stacked,
        };
        var name = new TextBlock
        {
            Text = image.Name,
            FontSize = 11,
            TextTrimming = TextTrimming.CharacterEllipsis,
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var content = new StackPanel { Width = TileSize, Spacing = 3 };
        content.Children.Add(frame);
        content.Children.Add(name);

        var tile = new Button
        {
            Width = TileSize,
            Padding = new Thickness(0),
            Margin = new Thickness(0, 0, 8, 8),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            Content = content,
        };
        if (found is not null)
        {
            ToolTip.SetTip(tile, ImportEvidence.HoverText(found));
        }
        var path = image.LocalPath;
        tile.Click += (_, _) => _openImages(_images, path);
        EnableCoverDrag(tile, image.FileId);
        _loadImage(thumbnail, path);
        return tile;
    }

    private static void EnableCoverDrag(Control tile, string fileId)
    {
        Point? pressAt = null;
        var dragging = false;
        tile.PointerPressed += (_, e) =>
        {
            if (e.GetCurrentPoint(tile).Properties.IsLeftButtonPressed)
            {
                pressAt = e.GetPosition(tile);
                dragging = false;
            }
        };
        tile.PointerMoved += async (_, e) =>
        {
            if (pressAt is not { } start
                || dragging
                || !e.GetCurrentPoint(tile).Properties.IsLeftButtonPressed)
            {
                return;
            }
            var now = e.GetPosition(tile);
            if (Math.Abs(now.X - start.X) < 6
                && Math.Abs(now.Y - start.Y) < 6)
            {
                return;
            }
            dragging = true;
            var data = new DataTransfer();
            data.Add(DataTransferItem.Create(CoverDragFormat, fileId));
            await DragDrop.DoDragDropAsync(e, data, DragDropEffects.Copy);
        };
        tile.PointerReleased += (_, _) => pressAt = null;
    }
}
