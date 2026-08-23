using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The metadata section: where the folder's metadata comes from. The lookup ⇄
/// file-tags control is the question this section answers, so it is on the
/// pane whether or not anything has been picked — not a link inside the
/// search.
/// </summary>
public sealed class ImportIdentitySectionTests
{
    [AvaloniaFact]
    public void TheReleaseUnknownControlIsThereBeforeAnythingIsPicked()
    {
        var section = Build(ImportIdentity.Release);

        Assert.Equal(
            new[] { Loc.Core("ui.import.metadata.lookup"), Loc.Core("ui.import.metadata.file_tags") },
            Buttons(section).Take(2).Select(button => button.Content as string).ToArray());
        // Nothing settled means no release card — the folder line at the top
        // of the pane already says what this is, and the search editor below
        // the section is where a release gets found.
        Assert.DoesNotContain(
            Buttons(section),
            button => Equals(button.Content, Loc.Core("ui.import.header.change_release")));
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
    // else — and they fold away inside the card that states what they add up
    // to, not on a line of their own beside it.
    [AvaloniaFact]
    public void TheReleaseFieldsFoldAwayInsideTheCard()
    {
        Assert.Empty(Build(ImportIdentity.Release).GetLogicalDescendants().OfType<Expander>());

        var settled = Build(ImportIdentity.Release, edit: Edit());

        var expander = Assert.Single(settled.GetLogicalDescendants().OfType<Expander>());
        var card = settled
            .GetLogicalDescendants()
            .OfType<Border>()
            .First(border => border.GetLogicalDescendants().OfType<TextBlock>()
                .Any(text => text.Text == "Album Title"));
        Assert.Contains(expander, card.GetLogicalDescendants());
    }

    // The card says what identified the release, as a badge — a statement, not
    // a control. A release a typed search turned up has no evidence about the
    // disc, so it draws no badge rather than an empty one.
    [AvaloniaFact]
    public void TheCardBadgesWhatIdentifiedTheRelease()
    {
        var discId = Build(
            ImportIdentity.Release,
            edit: Edit(),
            evidence: new BridgeClaimEvidence.DiscIdAlone());
        Assert.Contains(Texts(discId), text => text == Loc.Chrome("signal.kind.disc_id"));

        var barcode = Build(
            ImportIdentity.Release,
            edit: Edit(),
            evidence: new BridgeClaimEvidence.Barcode());
        Assert.Contains(Texts(barcode), text => text == Loc.Chrome("signal.kind.barcode"));

        var searched = Build(
            ImportIdentity.Release,
            edit: Edit(),
            evidence: new BridgeClaimEvidence.Search());
        Assert.DoesNotContain(Texts(searched), text => text == Loc.Chrome("signal.kind.disc_id"));
        Assert.DoesNotContain(Texts(searched), text => text == Loc.Chrome("signal.kind.barcode"));

        // Nothing in the card sets what identified the release: it is a fact
        // about the lookup, not a choice.
        Assert.Empty(discId.GetLogicalDescendants().OfType<CheckBox>());
    }

    // Leaving a release field writes that one field, once, with what was typed.
    [AvaloniaFact]
    public void LeavingAReleaseFieldWritesThatField()
    {
        var written = new List<(BridgeCandidateEditField Field, string Value)>();
        var section = Build(
            ImportIdentity.Release,
            edit: Edit(),
            onEditField: (field, value) => written.Add((field, value)));

        var boxes = section.GetLogicalDescendants().OfType<TextBox>().ToList();
        var year = boxes.First(box => box.Text == "1996");
        year.Text = "2011";
        year.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Equal(
            new[] { (BridgeCandidateEditField.Year, "2011") },
            written);
    }

    private static BridgeRawReleaseEdit Edit() => new(
        "Album Title",
        "Artist Name",
        new BridgeRawPressingEdit("1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012"),
        Array.Empty<BridgeRawTrackEdit>());

    private static IReadOnlyList<string> Texts(Control section) =>
        section.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();

    private static Control Build(
        ImportIdentity identity,
        bool isReading = false,
        BridgeRawReleaseEdit? edit = null,
        BridgeClaimEvidence? evidence = null,
        System.Action<ImportIdentity>? onSetIdentity = null,
        System.Action<BridgeCandidateEditField, string>? onEditField = null) =>
        new ImportIdentitySection
        {
            Identity = identity,
            HasSettled = edit is not null,
            CommitRow = null,
            Title = "Album Title",
            Edit = edit,
            MetaLine = "CD · 1996",
            Evidence = evidence,
            HasPick = evidence is not null,
            IsReading = isReading,
            LoadCover = null,
            HasCoverOptions = false,
            OnSetIdentity = onSetIdentity ?? (_ => { }),
            OnFindRelease = () => { },
            OnEditCover = () => { },
            OnEditField = onEditField ?? ((_, _) => { }),
        }.Build();

    private static IReadOnlyList<Button> Buttons(Control section) =>
        section.GetLogicalDescendants().OfType<Button>().ToList();
}
