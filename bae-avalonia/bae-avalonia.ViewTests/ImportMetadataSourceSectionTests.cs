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

public sealed class ImportMetadataSourceSectionTests
{
    [AvaloniaFact]
    public void PickerPresentsEveryModeWithoutSelectingMetadata()
    {
        var presented = new List<BridgeImportMetadataMode>();
        var section = Build(onPresentMode: presented.Add);

        foreach (var button in Buttons(section).Take(3))
        {
            button.RaiseEvent(new RoutedEventArgs(Avalonia.Controls.Button.ClickEvent));
        }

        Assert.Equal(
            new[]
            {
                BridgeImportMetadataMode.Lookup,
                BridgeImportMetadataMode.FileTags,
                BridgeImportMetadataMode.Manual,
            },
            presented);
    }

    [AvaloniaFact]
    public void FileTagsRequiresAnExplicitReadAndUse()
    {
        var reads = 0;
        var uses = 0;
        var presented = 0;
        var beforeRead = Build(
            mode: BridgeImportMetadataMode.FileTags,
            onPresentMode: _ => presented++,
            onReadFileTags: () => reads++);

        FindContentButton(beforeRead, Loc.Core("ui.import.metadata.file_tags"))
            .RaiseEvent(new RoutedEventArgs(Avalonia.Controls.Button.ClickEvent));

        var loaded = Build(
            mode: BridgeImportMetadataMode.FileTags,
            fileTagsPreview: FileTagsEdit(),
            onUseFileTags: () => uses++);
        FindContentButton(loaded, Loc.Chrome("import.metadata.use_file_tags"))
            .RaiseEvent(new RoutedEventArgs(Avalonia.Controls.Button.ClickEvent));

        Assert.Equal(1, reads);
        Assert.Equal(1, uses);
        Assert.Equal(0, presented);
    }

    [AvaloniaFact]
    public void ManualRequiresAnExplicitSelectionAndDoesNotOfferLookup()
    {
        var entered = 0;
        var section = Build(
            mode: BridgeImportMetadataMode.Manual,
            onEnterManually: () => entered++);

        FindContentButton(section, Loc.Chrome("import.metadata.enter_manually"))
            .RaiseEvent(new RoutedEventArgs(Avalonia.Controls.Button.ClickEvent));

        Assert.Equal(1, entered);
        Assert.DoesNotContain(
            Buttons(section),
            button => Equals(button.Content, Loc.Core("ui.import.header.find_release")));
    }

    [AvaloniaFact]
    public void UnselectedManualCardDoesNotUseTheFolderNameAsAlbumMetadata()
    {
        var section = Build(mode: BridgeImportMetadataMode.Manual);

        Assert.Contains(
            section.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.FontSize == 16
                && text.Text == Loc.Core("ui.import.slots.untitled"));
        Assert.DoesNotContain(
            section.GetLogicalDescendants().OfType<TextBlock>(),
            text => text.FontSize == 16 && text.Text == "Album Title");
    }

    [AvaloniaFact]
    public void SelectedMetadataFieldsWriteTheirTypedValues()
    {
        var written = new List<(BridgeCandidateEditField Field, string Value)>();
        var section = Build(
            hasSelectedSeed: true,
            edit: Edit(),
            onEditField: (field, value) => written.Add((field, value)));

        var year = section.GetLogicalDescendants().OfType<TextBox>()
            .First(box => box.Text == "1996");
        year.Text = "2011";
        year.RaiseEvent(new RoutedEventArgs(InputElement.LostFocusEvent));

        Assert.Equal(
            new[] { (BridgeCandidateEditField.Year, "2011") },
            written);
    }

    private static BridgeRawReleaseEdit Edit() => new(
        "Album Title",
        new BridgeArtistAssignment[]
        {
            new BridgeArtistAssignment.New(
                new BridgeNewArtistSeed("Artist Name", null, null, null)),
        },
        new BridgeRawPressingEdit("1996", "CD", "Label Name", "CAT-1", "UK", "0123456789012"),
        Array.Empty<BridgeRawTrackEdit>());

    private static BridgeReleaseUserEdit FileTagsEdit() => new(
        "Album Title",
        new BridgeArtistAssignment[]
        {
            new BridgeArtistAssignment.New(
                new BridgeNewArtistSeed("Artist Name", null, null, null)),
        },
        new BridgePressingEdit(1996, "CD", "Label Name", "CAT-1", "UK", "0123456789012"),
        Array.Empty<BridgeTrackUserEdit>());

    private static Control Build(
        BridgeImportMetadataMode mode = BridgeImportMetadataMode.Lookup,
        bool hasSelectedSeed = false,
        BridgeRawReleaseEdit? edit = null,
        BridgeReleaseUserEdit? fileTagsPreview = null,
        Action<BridgeImportMetadataMode>? onPresentMode = null,
        Action? onReadFileTags = null,
        Action? onUseFileTags = null,
        Action? onEnterManually = null,
        Action<BridgeCandidateEditField, string>? onEditField = null) =>
        new ImportMetadataSourceSection
        {
            Mode = mode,
            HasSelectedSeed = hasSelectedSeed,
            Title = "Album Title",
            Edit = edit,
            MetaLine = "CD · 1996",
            PickedSource = null,
            IsReading = false,
            FileTagsPreview = fileTagsPreview,
            FileTagsMetaLine = "CD · 1996",
            FileTagsError = null,
            LookupOptions = null,
            LoadCover = null,
            HasCoverOptions = false,
            CommitRow = null,
            Library = new LibraryService(),
            OnPresentMode = onPresentMode ?? (_ => { }),
            OnFindRelease = () => { },
            OnReadFileTags = onReadFileTags ?? (() => { }),
            OnUseFileTags = onUseFileTags ?? (() => { }),
            OnEnterManually = onEnterManually ?? (() => { }),
            OnEditCover = () => { },
            OnEditField = onEditField ?? ((_, _) => { }),
            OnEditArtists = _ => { },
        }.Build();

    private static Button FindContentButton(Control section, string label) =>
        Assert.Single(
            Buttons(section).Skip(3),
            button => Equals(button.Content, label));

    private static IReadOnlyList<Button> Buttons(Control section) =>
        section.GetLogicalDescendants().OfType<Button>().ToList();
}
