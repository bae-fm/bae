using System;
using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
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

    private readonly IReadOnlyList<BridgeMappingImage> _images;
    private readonly Action<Image, string> _loadImage;
    private readonly Action<IReadOnlyList<BridgeMappingImage>, string> _openImages;

    internal ImportMappingGallery(
        IReadOnlyList<BridgeMappingImage> images,
        Action<Image, string> loadImage,
        Action<IReadOnlyList<BridgeMappingImage>, string> openImages)
    {
        _images = images;
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
        var frame = new Border
        {
            Width = TileSize,
            Height = TileSize,
            CornerRadius = new CornerRadius(4),
            ClipToBounds = true,
            Child = thumbnail,
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
        var path = image.LocalPath;
        tile.Click += (_, _) => _openImages(_images, path);
        _loadImage(thumbnail, path);
        return tile;
    }
}
