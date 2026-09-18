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
/// The names read off an object are one answer drawn in three places: under
/// the import pane's audio facts, in a candidate row's hover, and behind the
/// library expansion's facts line.
/// </summary>
public sealed class ReleaseMarksTests
{
    [AvaloniaFact]
    public void EverySourceChipRequestsItsOwnMarkAndOrigin()
    {
        var requests = new List<BridgeEvidenceSelection>();
        var lines = MarkLines.Build(Marks, openEvidence: requests.Add);
        foreach (var chip in lines.GetLogicalDescendants().OfType<Button>())
            chip.RaiseEvent(new Avalonia.Interactivity.RoutedEventArgs(Button.ClickEvent));

        Assert.Equal<BridgeEvidenceSelection>([
            new BridgeEvidenceSelection.Mark(BridgeMarkKind.DiscId, DiscId, BridgeSignalOrigin.DiscToc),
            new BridgeEvidenceSelection.Mark(BridgeMarkKind.Barcode, "0075678164521", BridgeSignalOrigin.Artwork),
            new BridgeEvidenceSelection.Mark(BridgeMarkKind.Barcode, "0075678164521", BridgeSignalOrigin.CueSheet),
        ], requests);
    }

    [AvaloniaFact]
    public void OnlyTheCorroboratedBarcodeHasASeal()
    {
        var lines = MarkLines.Build([
            new BridgeReleaseMark(BridgeMarkKind.Barcode, "1234567890001", [BridgeSignalOrigin.Artwork], false),
            new BridgeReleaseMark(BridgeMarkKind.Barcode, "1234567890002", [BridgeSignalOrigin.Artwork], true),
        ]);
        var seals = lines.GetLogicalDescendants().OfType<Control>()
            .Where(control => Avalonia.Automation.AutomationProperties.GetName(control)
                == Loc.Core("core.identity.identified")).ToList();
        Assert.Equal(new double[] { 0, 1 }, seals.Select(seal => seal.Opacity));
        Assert.Equal(Avalonia.Automation.AccessibilityView.Raw,
            Avalonia.Automation.AutomationProperties.GetAccessibilityView(seals[0]));
    }

    [AvaloniaTheory]
    [InlineData(false)]
    [InlineData(true)]
    public void TheLibraryHoverSealsOnlyAnIdentifiedRelease(bool identified)
    {
        var line = new ReleaseFactsLine();
        line.Show("2003 · CD", identified ? BridgeMarkKind.Barcode : null, Marks, null, []);
        var button = Assert.IsType<Button>(line.Content);
        var seal = button.GetLogicalDescendants().OfType<Control>()
            .Single(control => Avalonia.Automation.AutomationProperties.GetName(control)
                == Loc.Core("core.identity.identified"));
        seal.Transitions = null;
        button.RaiseEvent(new Avalonia.Input.PointerEventArgs(
            Avalonia.Input.InputElement.PointerEnteredEvent, button,
            new Avalonia.Input.Pointer(1, Avalonia.Input.PointerType.Mouse, true),
            button, default, 0, default, default));
        Assert.Equal(identified ? 1 : 0, seal.Opacity);
    }

    [AvaloniaFact]
    public void AnExtractedIdentifierWithoutAMatchHasNoSeal()
    {
        var lines = MarkLines.Build(Marks);
        var seals = lines.GetLogicalDescendants()
            .OfType<Control>()
            .Where(control => Avalonia.Automation.AutomationProperties.GetName(control)
                == Loc.Core("core.identity.identified"))
            .ToList();

        Assert.NotEmpty(seals);
        Assert.All(seals, seal => Assert.Equal(0, seal.Opacity));
    }

    // Each line states what kind of name it is, the value as it was read, and
    // a tag for every surface it was read from.
    [AvaloniaFact]
    public void EveryMarkDrawsItsKindValueAndSurfaces()
    {
        var text = TextOf(MarkLines.Build(Marks));

        Assert.Contains(
            Loc.Core(BaeBridgeMethods.BridgeMarkKindKey(BridgeMarkKind.DiscId)),
            text);
        // A disc ID outruns the line, so its middle gives way and both ends
        // stay — an end-ellipsis would hide the half that tells two discs
        // apart.
        Assert.Contains("Wn8eRBtfL…GGMLhxxfM-", text);
        Assert.DoesNotContain(DiscId, text);
        Assert.Contains(
            Loc.Core(BaeBridgeMethods.BridgeMarkKindKey(BridgeMarkKind.Barcode)),
            text);
        // A barcode fits whole.
        Assert.Contains("0075678164521", text);
        foreach (var origin in new[]
        {
            BridgeSignalOrigin.DiscToc,
            BridgeSignalOrigin.Artwork,
            BridgeSignalOrigin.CueSheet,
        })
        {
            Assert.Contains(
                Loc.Core(BaeBridgeMethods.BridgeSignalOriginKey(origin)),
                text);
        }
    }

    // The pane states the folder's names; a candidate nothing was read off
    // draws none.
    [AvaloniaFact]
    public void ThePaneDrawsTheFoldersNamesAndNothingWithoutThem()
    {
        var withMarks = ImportMetadataSection(Marks);
        Assert.Contains("0075678164521", TextOf(withMarks));

        var without = ImportMetadataSection([]);
        Assert.DoesNotContain("0075678164521", TextOf(without));
    }

    // The glyphs' card leads with the folder's names, above the catalogs.
    [AvaloniaFact]
    public void TheGlyphCardLeadsWithTheFoldersNames()
    {
        var text = TextOf(ReleaseFactsFlyout.Build(Marks, null, Records));

        Assert.Contains("0075678164521", text);
        Assert.Contains(
            text,
            line => line.StartsWith(
                BaeBridgeMethods.BridgeCatalogName(BridgeCatalog.MusicBrainz)));
    }

    // The library expansion's facts line is a trigger for a release that
    // states a name of its own, even when no catalog describes it.
    [AvaloniaFact]
    public void TheFactsLineTriggersOnTheReleasesOwnNames()
    {
        var line = new ReleaseFactsLine();

        line.Show("2003 · CD", null, Marks, null, []);

        Assert.IsAssignableFrom<Button>(line.Content);
    }

    private static Control ImportMetadataSection(
        IReadOnlyList<BridgeReleaseMark> marks) =>
        new ImportMetadataSourceSection
        {
            Presentation = ImportMetadataPresentation.Draft,
            DraftIsBlank = false,
            CommitRow = null,
            Title = "Album Title",
            Edit = ImportCandidateFixtures.BlankEdit(),
            MetaLine = "2 tracks",
            SourceAudioLine = "FLAC · 44.1 kHz",
            Marks = marks,
            Verification = null,
            Records = [],
            FieldProvenance = ImportCandidateFixtures.FieldProvenance(),
            IsReading = false,
            LookupOptions = null,
            LoadCover = null,
            HasCoverOptions = false,
            Library = null!,
            OnPresent = _ => { },
            OnIdentify = () => { },
            OnSearchForRelease = () => { },
            OnResetToTags = () => { },
            OnClearMetadata = () => { },
            OnEditCover = () => { },
            OnSelectCover = _ => { },
            OnEditField = (_, _) => { },
            OnEditArtists = _ => { },
        }.Build();

    private const string DiscId = "Wn8eRBtfLDMmvbjEACGGMLhxxfM-";

    private static readonly BridgeReleaseMark[] Marks =
    [
        new BridgeReleaseMark(
            BridgeMarkKind.DiscId,
            DiscId,
            [BridgeSignalOrigin.DiscToc],
            false),
        new BridgeReleaseMark(
            BridgeMarkKind.Barcode,
            "0075678164521",
            [BridgeSignalOrigin.Artwork, BridgeSignalOrigin.CueSheet],
            false),
    ];

    private static readonly BridgeReleaseRecord[] Records =
    [
        new BridgeReleaseRecord(
            BridgeCatalog.MusicBrainz,
            "rel-1",
            "https://musicbrainz.org/release/rel-1",
            true),
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
