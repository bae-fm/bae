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
    public void BlankDraftOffersBothPrefillSourcesAndEditableFields()
    {
        var presentations = new List<ImportMetadataPresentation>();
        var section = Build(
            draftIsBlank: true,
            onPresent: presentations.Add);

        Click(section, Loc.Chrome("import.metadata.find_online_ellipsis"));
        Click(section, Loc.Core("ui.import.metadata.file_tags") + "…");

        Assert.Equal(
            new[]
            {
                ImportMetadataPresentation.FindOnline,
                ImportMetadataPresentation.FileTags,
            },
            presentations);
        Assert.Contains(
            section.GetLogicalDescendants().OfType<TextBox>(),
            field => field.Text == "Album Title");
    }

    [AvaloniaFact]
    public void FileTagsPreviewAppliesTheDisplayedSource()
    {
        var applications = 0;
        var section = Build(
            presentation: ImportMetadataPresentation.FileTags,
            fileTagsPreview: FileTagsEdit(),
            onUseFileTags: () => applications++);

        Click(section, Loc.Chrome("import.metadata.apply"));

        Assert.Equal(1, applications);
        Assert.Contains("Album Title", Texts(section));
    }

    [AvaloniaFact]
    public void FileTagsReadShowsProgress()
    {
        var section = Build(
            presentation: ImportMetadataPresentation.FileTags,
            isReading: true);

        Assert.Single(section.GetLogicalDescendants().OfType<Spinner>());
    }

    [AvaloniaFact]
    public void DraftFieldsWriteTheirTypedValues()
    {
        var written = new List<(BridgeCandidateEditField Field, string Value)>();
        var section = Build(
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
        ImportMetadataPresentation presentation = ImportMetadataPresentation.Draft,
        bool draftIsBlank = false,
        bool isReading = false,
        BridgeReleaseUserEdit? fileTagsPreview = null,
        Action<ImportMetadataPresentation>? onPresent = null,
        Action? onUseFileTags = null,
        Action<BridgeCandidateEditField, string>? onEditField = null) =>
        new ImportMetadataSourceSection
        {
            Presentation = presentation,
            DraftIsBlank = draftIsBlank,
            Title = "Album Title",
            Edit = Edit(),
            MetaLine = "CD · 1996",
            ProvenanceLabel = null,
            ProvenanceUri = null,
            IsReading = isReading,
            FileTagsPreview = fileTagsPreview,
            FileTagsMetaLine = "CD · 1996",
            FileTagsError = null,
            LookupOptions = new TextBlock { Text = "Search form" },
            LoadCover = null,
            HasCoverOptions = false,
            CommitRow = null,
            Library = new LibraryService(),
            OnPresent = onPresent ?? (_ => { }),
            OnReadFileTags = () => { },
            OnUseFileTags = onUseFileTags ?? (() => { }),
            OnClearMetadata = () => { },
            OnEditCover = () => { },
            OnSelectCover = _ => { },
            OnEditField = onEditField ?? ((_, _) => { }),
            OnEditArtists = _ => { },
        }.Build();

    private static void Click(Control section, string label) =>
        Assert.Single(
            section.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, label))
        .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static IReadOnlyList<string> Texts(Control section) =>
        section.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();
}
