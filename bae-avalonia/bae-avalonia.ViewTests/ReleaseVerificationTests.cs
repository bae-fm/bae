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
/// What the rip databases said is one line drawn under the names the object
/// states: in the import pane, in a candidate row's hover, and behind the
/// library expansion's facts line.
/// </summary>
public sealed class ReleaseVerificationTests
{
    // The line states the count in the words the locale renders it with, so
    // the surface never composes the sentence itself.
    [AvaloniaFact]
    public void TheLineStatesHowManyOtherRipsMatch()
    {
        var line = RipMatchLine.Build(Verified);

        Assert.NotNull(line);
        Assert.Contains(Matches37, TextOf(line!));
    }

    // A release whose every track no database confirmed has no count, and a
    // line that states nothing is no line.
    [AvaloniaFact]
    public void ARipNothingConfirmedDrawsNoLine()
    {
        Assert.Null(RipMatchLine.Build(Unconfirmed));
        Assert.Null(RipMatchLine.Build(null));
    }

    // The pane states it once, under the names the folder carries.
    [AvaloniaFact]
    public void ThePaneDrawsTheLineAndNothingWithoutIt()
    {
        Assert.Contains(Matches37, TextOf(ImportMetadataSection(Verified)));
        Assert.DoesNotContain(Matches37, TextOf(ImportMetadataSection(null)));
    }

    // The row's hover states it with the names, above the catalogs.
    [AvaloniaFact]
    public void TheIdentifiedHoverStatesWhatTheDatabasesSaid()
    {
        var text = TextOf(IdentifiedFromFlyout.Build([], Verified, Records));

        Assert.Contains(Matches37, text);
    }

    // The library expansion's facts line is a trigger for a release the rip
    // databases confirmed, even when nothing was read off its folder and no
    // catalog describes it.
    [AvaloniaFact]
    public void TheFactsLineTriggersOnTheVerificationAlone()
    {
        var line = new ReleaseFactsLine();

        line.Show("2003 · CD", [], Verified, []);

        Assert.IsAssignableFrom<Button>(line.Content);
    }

    private static string Matches37 => Loc.Core(
        "core.verification.matches_other_rips",
        "count",
        37L);

    private static Control ImportMetadataSection(BridgeVerification? verification) =>
        new ImportMetadataSourceSection
        {
            Presentation = ImportMetadataPresentation.Draft,
            DraftIsBlank = false,
            CommitRow = null,
            Title = "Album Title",
            Edit = ImportCandidateFixtures.BlankEdit(),
            MetaLine = "2 tracks",
            SourceAudioLine = "FLAC · 44.1 kHz",
            Marks = [],
            Verification = verification,
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

    private static readonly BridgeVerification Verified = new(
        BridgeVerificationSource.Log,
        37,
        [new BridgeTrackVerification(1, 42, 16, 0xE94F69D5)]);

    private static readonly BridgeVerification Unconfirmed = new(
        BridgeVerificationSource.Log,
        null,
        [new BridgeTrackVerification(1, null, null, null)]);

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
