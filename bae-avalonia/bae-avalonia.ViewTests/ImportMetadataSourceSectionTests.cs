using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Layout;
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
    public void BlankDraftDoesNotRenderAReleaseSummaryPlaceholder()
    {
        var section = Build(
            draftIsBlank: true,
            title: Loc.Chrome("import.metadata.album_title_placeholder"),
            edit: BlankEdit());

        Assert.DoesNotContain(
            Loc.Chrome("import.metadata.album_title_placeholder"),
            Texts(section));
        Assert.Contains(
            section.GetLogicalDescendants().OfType<TextBox>(),
            field => field.Text == string.Empty);

        var card = Assert.IsType<Border>(section);
        var body = Assert.IsType<StackPanel>(card.Child);
        var layout = Assert.IsType<Grid>(Assert.Single(body.Children));
        Assert.Equal(2, layout.ColumnDefinitions.Count);
        var editor = Assert.IsType<StackPanel>(
            Assert.Single(layout.Children, child => Grid.GetColumn(child) == 1));
        Assert.NotEmpty(editor.GetLogicalDescendants().OfType<TextBox>());
    }

    [AvaloniaFact]
    public void AppliedDraftKeepsEveryMetadataActionVisible()
    {
        var presentations = new List<ImportMetadataPresentation>();
        var clears = 0;
        var section = Build(
            draftIsBlank: false,
            onPresent: presentations.Add,
            onClearMetadata: () => clears++);

        Click(section, Loc.Chrome("import.metadata.find_online_ellipsis"));
        Click(section, Loc.Core("ui.import.metadata.file_tags") + "…");
        Click(section, Loc.Chrome("import.metadata.clear"));

        Assert.Equal(
            new[]
            {
                ImportMetadataPresentation.FindOnline,
                ImportMetadataPresentation.FileTags,
            },
            presentations);
        Assert.Equal(1, clears);
    }

    [AvaloniaFact]
    public void AppliedDraftKeepsDetailsWithMetadataAndClearApartFromSources()
    {
        var section = Build(draftIsBlank: false);
        var details = Assert.Single(
            section.GetLogicalDescendants().OfType<Expander>());
        var metadataColumn = Assert.IsType<StackPanel>(
            details.GetLogicalParent());

        Assert.Equal(1, Grid.GetColumn(metadataColumn));

        var findOnline = ButtonNamed(
            section,
            Loc.Chrome("import.metadata.find_online_ellipsis"));
        var fileTags = ButtonNamed(
            section,
            Loc.Core("ui.import.metadata.file_tags") + "…");
        var clear = ButtonNamed(section, Loc.Chrome("import.metadata.clear"));

        Assert.Same(findOnline.GetLogicalParent(), fileTags.GetLogicalParent());
        Assert.NotSame(findOnline.GetLogicalParent(), clear.GetLogicalParent());
        Assert.Equal(HorizontalAlignment.Right, clear.HorizontalAlignment);
    }

    [AvaloniaFact]
    public void DraftRendersTheSourceAudioLine()
    {
        var section = Build(sourceAudioLine: "FLAC · 44.1 kHz · 16-bit · stereo");

        Assert.Contains("FLAC · 44.1 kHz · 16-bit · stereo", Texts(section));
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

    private static BridgeRawReleaseEdit BlankEdit() => new(
        string.Empty,
        Array.Empty<BridgeArtistAssignment>(),
        new BridgeRawPressingEdit(
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty,
            string.Empty),
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
        Action? onClearMetadata = null,
        Action<BridgeCandidateEditField, string>? onEditField = null,
        string? title = null,
        BridgeRawReleaseEdit? edit = null,
        string sourceAudioLine = "FLAC · 44.1 kHz · 16-bit · stereo") =>
        new ImportMetadataSourceSection
        {
            Presentation = presentation,
            DraftIsBlank = draftIsBlank,
            Title = title ?? "Album Title",
            Edit = edit ?? Edit(),
            MetaLine = "CD · 1996",
            SourceAudioLine = sourceAudioLine,
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
            OnClearMetadata = onClearMetadata ?? (() => { }),
            OnEditCover = () => { },
            OnSelectCover = _ => { },
            OnEditField = onEditField ?? ((_, _) => { }),
            OnEditArtists = _ => { },
        }.Build();

    private static void Click(Control section, string label) =>
        ButtonNamed(section, label)
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static Button ButtonNamed(Control section, string label) =>
        Assert.Single(
            section.GetLogicalDescendants().OfType<Button>(),
            button => Equals(button.Content, label));

    private static IReadOnlyList<string> Texts(Control section) =>
        section.GetLogicalDescendants().OfType<TextBlock>()
            .Select(text => text.Text ?? string.Empty).ToList();
}
