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

    private static Control Build(
        ImportIdentity identity,
        bool isReading = false,
        BridgeRawPressingEdit? pressing = null,
        System.Action<ImportIdentity>? onSetIdentity = null) =>
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
            Claim = null,
            HasPick = false,
            IsReading = isReading,
            LoadCover = null,
            HasCoverOptions = false,
            Pressing = pressing,
            OnSetIdentity = onSetIdentity ?? (_ => { }),
            OnFindRelease = () => { },
            OnEditCover = () => { },
            OnAlbumTitle = _ => { },
            OnAlbumArtist = _ => { },
            OnPressing = _ => { },
        }.Build();

    private static IReadOnlyList<Button> Buttons(Control section) =>
        section.GetLogicalDescendants().OfType<Button>().ToList();
}
