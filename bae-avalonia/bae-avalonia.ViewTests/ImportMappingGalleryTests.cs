using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.Layout;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ImportMappingGalleryTests
{
    [AvaloniaFact]
    public void GalleryUsesFixedTilesAndOpensTheOrderedLightbox()
    {
        var loaded = new List<string>();
        var opened = new List<(string[] Files, string Path)>();
        var images = Images();
        var gallery = new ImportMappingGallery(
            images,
            (_, path) => loaded.Add(path),
            (all, path) => opened.Add((all.Select(image => image.FileId).ToArray(), path)))
            .Build();

        var panel = Assert.IsType<WrapPanel>(gallery);
        var tiles = panel.Children.OfType<Button>().ToList();
        var thumbnails = panel.GetLogicalDescendants().OfType<Image>().ToList();
        var coverMarkers = panel.GetLogicalDescendants().OfType<TextBlock>()
            .Where(text => text.Text == Loc.Core("ui.import.becomes.cover"))
            .ToList();

        Assert.Equal(HorizontalAlignment.Stretch, panel.HorizontalAlignment);
        Assert.All(tiles, tile => Assert.Equal(ImportMappingGallery.TileSize, tile.Width));
        Assert.All(thumbnails, image =>
        {
            Assert.Equal(ImportMappingGallery.TileSize, image.Width);
            Assert.Equal(ImportMappingGallery.TileSize, image.Height);
        });
        Assert.Equal(new[] { "/folder/front.jpg", "/folder/back.jpg" }, loaded);
        Assert.Equal(2, coverMarkers.Count);
        Assert.Single(coverMarkers, marker => marker.Opacity == 1);

        tiles[1].RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Single(opened);
        Assert.Equal(new[] { "front.jpg", "back.jpg" }, opened[0].Files);
        Assert.Equal("/folder/back.jpg", opened[0].Path);
    }

    private static BridgeMappingImage[] Images() =>
    [
        new BridgeMappingImage(
            FileId: "front.jpg",
            Name: "front.jpg",
            Size: 2048,
            LocalPath: "/folder/front.jpg",
            IsCover: true),
        new BridgeMappingImage(
            FileId: "back.jpg",
            Name: "back.jpg",
            Size: 1024,
            LocalPath: "/folder/back.jpg",
            IsCover: false),
    ];
}
