using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// Which catalogs describe a release is one answer, drawn once: as a row of
/// links last in the import pane, and behind the library expansion's facts
/// line.
/// </summary>
public sealed class ReleaseRecordsTests
{
    // The row names every catalog that describes the release, whichever of
    // them the draft was read from.
    [AvaloniaFact]
    public void TheRecordsRowNamesEveryCatalog()
    {
        var records = BaeBridgeMethods
            .BridgeCatalogs()
            .Select(catalog => new BridgeReleaseRecord(
                catalog,
                "https://example.test/key-1"))
            .ToList();

        var text = TextOf(ReleaseRecordsRow.Build(records));

        foreach (var catalog in BaeBridgeMethods.BridgeCatalogs())
        {
            var name = BaeBridgeMethods.BridgeCatalogName(catalog);
            Assert.Contains(text, line => line.Contains(name));
        }
    }

    // The library expansion's facts line is a trigger exactly when a catalog
    // describes the release; with none it is the line it always was.
    [AvaloniaFact]
    public void TheFactsLineIsATriggerOnlyWhenACatalogDescribesTheRelease()
    {
        var line = new ReleaseFactsLine();

        line.Show("2003 · CD", []);
        Assert.IsAssignableFrom<TextBlock>(line.Content);

        line.Show("2003 · CD", Records);
        Assert.IsAssignableFrom<Button>(line.Content);
    }

    private static readonly BridgeReleaseRecord[] Records =
    [
        new BridgeReleaseRecord(
            BridgeCatalog.MusicBrainz,
            "https://musicbrainz.org/release/rel-1"),
        new BridgeReleaseRecord(
            BridgeCatalog.Discogs,
            "https://www.discogs.com/release/4242"),
    ];

    private static List<string> TextOf(Control root) =>
        root
            .GetLogicalDescendants()
            .OfType<TextBlock>()
            .Select(block => block.Text)
            .Where(value => !string.IsNullOrEmpty(value))
            .Select(value => value!)
            .ToList();
}
