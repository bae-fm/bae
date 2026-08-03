using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The identity section: what the folder is being read as. The release ⇄
/// Unknown control is the question this section answers, so it is on the pane
/// whether or not anything has been picked — not a link inside the search.
/// </summary>
public sealed class ImportIdentitySectionTests
{
    [AvaloniaFact]
    public void TheReleaseUnknownControlIsThereBeforeAnythingIsPicked()
    {
        var section = Build(ImportIdentity.Release);

        Assert.Equal(
            new[] { Loc.Core("ui.import.identity.release"), Loc.Core("ui.import.identity.unknown") },
            Buttons(section).Take(2).Select(button => button.Content as string).ToArray());
        // Nothing settled means no release card — the folder line above
        // already says what this is, and the search editor below the section
        // is where a release gets found. The folder stays named throughout.
        Assert.DoesNotContain(
            Buttons(section),
            button => Equals(button.Content, Loc.Core("ui.import.header.change_release")));
        Assert.Contains(
            section.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.Text == "Folder Name");
    }

    [AvaloniaFact]
    public void EitherSideOfTheControlSwitchesToIt()
    {
        var chosen = new List<ImportIdentity>();
        var section = Build(ImportIdentity.Release, onSetIdentity: chosen.Add);

        foreach (var button in Buttons(section).Take(2))
        {
            button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
        }

        Assert.Equal(new[] { ImportIdentity.Release, ImportIdentity.Unknown }, chosen);
    }

    // A read in flight leaves the section showing what it already has; the
    // controls that would start a second one go quiet.
    [AvaloniaFact]
    public void AReadInFlightDisablesTheControlsThatStartOne()
    {
        var section = Build(ImportIdentity.Release, isReading: true);

        Assert.All(Buttons(section), button => Assert.False(button.IsEnabled));
    }

    // The release's own fields are there to edit exactly when there is a release
    // to edit — before that the section states the open question and nothing
    // else.
    [AvaloniaFact]
    public void TheReleaseFieldsArriveWithSomethingSettled()
    {
        Assert.Empty(Build(ImportIdentity.Release).GetLogicalDescendants().OfType<Expander>());

        var settled = Build(
            ImportIdentity.Release,
            pressing: new BridgeRawPressingEdit("1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012"));

        Assert.Single(settled.GetLogicalDescendants().OfType<Expander>());
    }

    // The claim the card states is the user's to set: picking a release claims
    // that pressing, and the control beside the sentence is how they say they
    // hold the album but not necessarily this pressing.
    [AvaloniaFact]
    public void TheClaimLineOffersBothLevelsAndReportsTheOneClicked()
    {
        var set = new List<BridgeClaimLevel>();
        var section = Build(
            ImportIdentity.Release,
            pressing: new BridgeRawPressingEdit("1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012"),
            claim: Claim(BridgeClaimLevel.Exact),
            onSetClaimLevel: set.Add);

        var levels = Buttons(section)
            .Where(button => Equals(button.Content, Loc.Core("ui.import.claim.level.exact"))
                || Equals(button.Content, Loc.Core("ui.import.claim.level.album")))
            .ToList();
        Assert.Equal(2, levels.Count);
        foreach (var button in levels)
        {
            button.RaiseEvent(new RoutedEventArgs(Button.ClickEvent));
        }

        Assert.Equal(new[] { BridgeClaimLevel.Exact, BridgeClaimLevel.Approximate }, set);
    }

    // A pressing claim names the release inside its own sentence; only the
    // album claim leaves it unsaid, so only that one draws the second line.
    [AvaloniaFact]
    public void OnlyTheAlbumClaimNamesWhereTheMetadataCameFrom()
    {
        var pressing = new BridgeRawPressingEdit("1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012");
        var exact = Build(ImportIdentity.Release, pressing: pressing, claim: Claim(BridgeClaimLevel.Exact));
        var album = Build(ImportIdentity.Release, pressing: pressing, claim: Claim(BridgeClaimLevel.Approximate));

        Assert.Contains(Texts(exact), text => text == Loc.Core("ui.import.claim.pressing", "release", "CD · 1996"));
        Assert.DoesNotContain(Texts(exact), text => text.StartsWith(MetadataFromPrefix()));

        Assert.Contains(Texts(album), text => text == Loc.Core("ui.import.claim.album"));
        Assert.Contains(Texts(album), text => text.StartsWith(MetadataFromPrefix()));
    }

    /// <summary>The metadata-from sentence with its release slot emptied, so the
    /// assertion matches the sentence rather than restating a translation.</summary>
    private static string MetadataFromPrefix() =>
        Loc.Core("ui.import.claim.metadata_from", "release", string.Empty).TrimEnd();

    private static BridgeClaimLine Claim(BridgeClaimLevel level) => new(
        Choice: level is BridgeClaimLevel.Exact
            ? new BridgeIdentityChoice.Exact("rel-1", BridgeMetadataSource.MusicBrainz)
            : new BridgeIdentityChoice.Approximate("rel-1", BridgeMetadataSource.MusicBrainz),
        Level: level,
        Evidence: new BridgeClaimEvidence.DiscIdAlone(),
        Release: "CD · 1996",
        TrackCount: 12);

    private static IReadOnlyList<string> Texts(Control section) =>
        section.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();

    private static Control Build(
        ImportIdentity identity,
        bool isReading = false,
        BridgeRawPressingEdit? pressing = null,
        BridgeClaimLine? claim = null,
        System.Action<ImportIdentity>? onSetIdentity = null,
        System.Action<BridgeClaimLevel>? onSetClaimLevel = null) =>
        new ImportIdentitySection
        {
            Identity = identity,
            FolderName = "Folder Name",
            FormatLabel = "FLAC",
            HasSettled = pressing is not null,
            CommitRow = null,
            Title = "Album Title",
            AlbumTitle = pressing is null ? string.Empty : "Album Title",
            AlbumArtistText = pressing is null ? string.Empty : "Artist Name",
            MetaLine = "CD · 1996",
            Claim = claim,
            HasPick = claim is not null,
            IsReading = isReading,
            LoadCover = null,
            HasCoverOptions = false,
            Pressing = pressing,
            OnSetIdentity = onSetIdentity ?? (_ => { }),
            OnSetClaimLevel = onSetClaimLevel ?? (_ => { }),
            OnFindRelease = () => { },
            OnEditCover = () => { },
            OnAlbumTitle = _ => { },
            OnAlbumArtist = _ => { },
            OnPressing = _ => { },
        }.Build();

    private static IReadOnlyList<Button> Buttons(Control section) =>
        section.GetLogicalDescendants().OfType<Button>().ToList();
}
