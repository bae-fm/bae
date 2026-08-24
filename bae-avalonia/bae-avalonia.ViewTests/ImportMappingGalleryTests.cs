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
            System.Array.Empty<BridgeFileEvidence>(),
            (_, path) => loaded.Add(path),
            (all, path) => opened.Add((all.Select(image => image.FileId).ToArray(), path)))
            .Build();

        var panel = Assert.IsType<WrapPanel>(gallery);
        var tiles = panel.Children.OfType<Button>().ToList();
        var thumbnails = panel.GetLogicalDescendants().OfType<Image>().ToList();
        Assert.Equal(HorizontalAlignment.Stretch, panel.HorizontalAlignment);
        Assert.All(tiles, tile => Assert.Equal(ImportMappingGallery.TileSize, tile.Width));
        Assert.All(thumbnails, image =>
        {
            Assert.Equal(ImportMappingGallery.TileSize, image.Width);
            Assert.Equal(ImportMappingGallery.TileSize, image.Height);
        });
        Assert.Equal(new[] { "/folder/front.jpg", "/folder/back.jpg" }, loaded);
        // The gallery lists what the folder has; which image leads the release
        // is the cover choice, and the card is where that shows.
        Assert.Equal(
            new[] { "front.jpg", "back.jpg" },
            panel.GetLogicalDescendants().OfType<TextBlock>().Select(text => text.Text));

        tiles[1].RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

        Assert.Single(opened);
        Assert.Equal(new[] { "front.jpg", "back.jpg" }, opened[0].Files);
        Assert.Equal("/folder/back.jpg", opened[0].Path);
    }

    // The image a barcode was read off says so, and only that image does. The
    // whole tile hovers to the sentence naming the code.
    [AvaloniaFact]
    public void TheImageTheReleaseWasIdentifiedFromSaysSo()
    {
        var evidence = new[]
        {
            new BridgeFileEvidence(
                BridgeEvidenceSignal.Barcode,
                Value: "5099969394522",
                FileId: "back.jpg"),
        };
        var gallery = new ImportMappingGallery(
            Images(),
            evidence,
            (_, _) => { },
            (_, _) => { })
            .Build();

        var tiles = Assert.IsType<WrapPanel>(gallery).Children.OfType<Button>().ToList();
        Assert.Null(ToolTip.GetTip(tiles[0]));
        Assert.Equal(
            Loc.Core(
                "core.import.evidence.barcode_in_image",
                "value",
                "5099969394522"),
            ToolTip.GetTip(tiles[1]));

        var labels = tiles[1].GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text).ToList();
        Assert.Contains(Loc.Chrome("signal.kind.barcode"), labels);
        Assert.DoesNotContain(
            Loc.Chrome("signal.kind.barcode"),
            tiles[0].GetLogicalDescendants().OfType<TextBlock>().Select(text => text.Text));
    }

    private static BridgeMappingImage[] Images() =>
    [
        new BridgeMappingImage(
            FileId: "front.jpg",
            Name: "front.jpg",
            Size: 2048,
            LocalPath: "/folder/front.jpg"),
        new BridgeMappingImage(
            FileId: "back.jpg",
            Name: "back.jpg",
            Size: 1024,
            LocalPath: "/folder/back.jpg"),
    ];
}
