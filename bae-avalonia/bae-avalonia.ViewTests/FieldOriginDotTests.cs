using System.Linq;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Headless.XUnit;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>What the dot after a release field says, and when it says
/// nothing.</summary>
public sealed class FieldOriginDotTests
{
    /// <summary>A field core marked draws a dot, and a field it marked nothing
    /// for draws none. Which of them are marked is core's answer.</summary>
    [AvaloniaFact]
    public void ADotIsDrawnForTheMarkedFieldsAndForNoOthers()
    {
        var marked = ImportCandidateFixtures.FieldProvenance()
            .Where(entry => FieldOriginDot.For(entry) is not null)
            .Select(entry => entry.Field)
            .ToArray();

        Assert.Equal(
            new[]
            {
                BridgeCandidateEditField.AlbumTitle,
                BridgeCandidateEditField.CatalogNumber,
            },
            marked);
    }

    /// <summary>The dot names its field, so a test or an assistive technology
    /// can say which value it stands after.</summary>
    [AvaloniaFact]
    public void TheDotNamesTheFieldItStandsAfter()
    {
        var entry = ImportCandidateFixtures.FieldProvenance()
            .Single(item => item.Field == BridgeCandidateEditField.CatalogNumber);
        var dot = Assert.IsType<Ellipse>(FieldOriginDot.For(entry));
        Assert.Equal("origin-dot-catalog_number", dot.Name);
    }

    /// <summary>What stands behind the dot: every catalog describing the
    /// release and what each one states.</summary>
    [AvaloniaFact]
    public void TheHoverListsBothCatalogsReadings()
    {
        var entry = ImportCandidateFixtures.FieldProvenance()
            .Single(item => item.Field == BridgeCandidateEditField.CatalogNumber);

        var lines = FieldOriginDot.Lines(entry)
            .GetLogicalDescendants()
            .OfType<TextBlock>()
            .Select(text => text.Text)
            .ToArray();

        Assert.Contains(
            BaeBridgeMethods.BridgeCatalogName(BridgeCatalog.MusicBrainz),
            lines);
        Assert.Contains(
            BaeBridgeMethods.BridgeCatalogName(BridgeCatalog.Discogs),
            lines);
        Assert.Contains("CAT-1", lines);
        Assert.Contains("CAT-1-A", lines);
    }

    /// <summary>A value a person typed says so under the readings, so the
    /// hover explains the grey dot rather than leaving it to be guessed
    /// at.</summary>
    [AvaloniaFact]
    public void ATypedValueSaysSoInItsHover()
    {
        var entry = ImportCandidateFixtures.FieldProvenance()
            .Single(item => item.Field == BridgeCandidateEditField.AlbumTitle);

        var lines = FieldOriginDot.Lines(entry)
            .GetLogicalDescendants()
            .OfType<TextBlock>()
            .Select(text => text.Text)
            .ToArray();

        Assert.Contains(Loc.Core("core.field.origin.typed"), lines);
    }

    /// <summary>The import pane's RELEASE grid puts the dots beside the values
    /// core marked, and beside no others.</summary>
    [AvaloniaFact]
    public void TheImportGridDrawsTheDotsCoreMarked()
    {
        var section = ImportMetadataSourceSectionTests.BuildSection();
        var window = new Window { Width = 900, Height = 900, Content = section };
        window.Show();
        try
        {
            var dots = section.GetLogicalDescendants()
                .OfType<Ellipse>()
                .Select(dot => dot.Name)
                .Where(name => name?.StartsWith("origin-dot-") == true)
                .ToArray();
            Assert.Equal(
                new[] { "origin-dot-album_title", "origin-dot-catalog_number" },
                dots);
        }
        finally
        {
            window.Close();
        }
    }
}
