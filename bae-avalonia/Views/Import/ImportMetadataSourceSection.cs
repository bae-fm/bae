using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>The metadata surface being inspected and the explicit action that
/// selects it for import.</summary>
internal sealed class ImportMetadataSourceSection
{
    internal required BridgeImportMetadataMode Mode { get; init; }
    internal required bool HasSelectedSeed { get; init; }
    internal required string Title { get; init; }
    internal required BridgeRawReleaseEdit? Edit { get; init; }
    internal required string MetaLine { get; init; }
    internal required BridgeMetadataSource? PickedSource { get; init; }
    internal required bool IsReading { get; init; }
    internal required BridgeReleaseUserEdit? FileTagsPreview { get; init; }
    internal required string FileTagsMetaLine { get; init; }
    internal required string? FileTagsError { get; init; }
    internal required Control? LookupOptions { get; init; }
    internal required Action<Image>? LoadCover { get; init; }
    internal required bool HasCoverOptions { get; init; }
    internal required Control? CommitRow { get; init; }
    internal required LibraryService Library { get; init; }
    internal required Action<BridgeImportMetadataMode> OnPresentMode { get; init; }
    internal required Action OnFindRelease { get; init; }
    internal required Action OnReadFileTags { get; init; }
    internal required Action OnUseFileTags { get; init; }
    internal required Action OnEnterManually { get; init; }
    internal required Action OnEditCover { get; init; }
    internal required Action<BridgeCandidateEditField, string> OnEditField { get; init; }
    internal required Action<IReadOnlyList<BridgeArtistAssignment>> OnEditArtists { get; init; }

    internal Control Build()
    {
        var column = new StackPanel { Spacing = 10 };
        column.Children.Add(ModePicker());
        column.Children.Add(ModeContent());
        return column;
    }

    private Control ModePicker()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        row.Children.Add(Segment(
            Loc.Core("ui.import.metadata.lookup"),
            BridgeImportMetadataMode.Lookup));
        row.Children.Add(Segment(
            Loc.Core("ui.import.metadata.file_tags"),
            BridgeImportMetadataMode.FileTags));
        row.Children.Add(Segment(
            Loc.Core("ui.import.metadata.manual"),
            BridgeImportMetadataMode.Manual));
        return row;
    }

    private Control Segment(string label, BridgeImportMetadataMode mode)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = !IsReading;
        if (Mode == mode)
        {
            button[!Button.BackgroundProperty] =
                new DynamicResourceExtension("BaeSelectionTintBrush");
            button[!Button.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextPrimaryBrush");
        }
        button.Click += (_, _) => OnPresentMode(mode);
        return button;
    }

    private Control ModeContent() => Mode switch
    {
        BridgeImportMetadataMode.Lookup => LookupContent(),
        BridgeImportMetadataMode.FileTags => FileTagsContent(),
        BridgeImportMetadataMode.Manual => ManualContent(),
        _ => throw new ArgumentOutOfRangeException(nameof(Mode), Mode, "Unknown metadata mode"),
    };

    private Control LookupContent()
    {
        if (HasSelectedSeed)
        {
            return SelectedCard(
                Loc.Core("ui.import.header.change_release"),
                OnFindRelease);
        }
        var column = new StackPanel { Spacing = 8 };
        if (LookupOptions is not null)
        {
            column.Children.Add(LookupOptions);
        }
        column.Children.Add(ActionButton(
            Loc.Core("ui.import.header.find_release"),
            OnFindRelease));
        return column;
    }

    private Control FileTagsContent()
    {
        if (HasSelectedSeed)
        {
            return SelectedCard(null, null);
        }
        if (FileTagsPreview is { } preview)
        {
            return Card(
                preview.AlbumTitle,
                ArtistAssignmentDisplay.Join(preview.AlbumArtistAssignments),
                FileTagsMetaLine,
                edit: null,
                source: null,
                actionLabel: Loc.Chrome("import.metadata.use_file_tags"),
                onAction: OnUseFileTags,
                includeSelectedValues: false);
        }
        var column = new StackPanel { Spacing = 6 };
        column.Children.Add(ActionButton(
            Loc.Core("ui.import.metadata.file_tags"),
            OnReadFileTags));
        if (FileTagsError is { Length: > 0 } error)
        {
            var line = DialogUi.Danger();
            line.Text = error;
            line.IsVisible = true;
            column.Children.Add(line);
        }
        return column;
    }

    private Control ManualContent()
    {
        if (HasSelectedSeed)
        {
            return SelectedCard(null, null);
        }
        return Card(
            Loc.Core("ui.import.slots.untitled"),
            string.Empty,
            Loc.Core("ui.import.metadata.manual"),
            edit: null,
            source: null,
            actionLabel: Loc.Chrome("import.metadata.enter_manually"),
            onAction: OnEnterManually,
            includeSelectedValues: false);
    }

    private Control SelectedCard(string? actionLabel, Action? onAction) =>
        Card(
            Title,
            Edit is null
                ? string.Empty
                : ArtistAssignmentDisplay.Join(Edit.AlbumArtistAssignments),
            MetaLine,
            Edit,
            PickedSource,
            actionLabel,
            onAction,
            includeSelectedValues: true);

    private Button ActionButton(string label, Action action)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = !IsReading;
        button.Click += (_, _) => action();
        return button;
    }

    private Control Card(
        string titleText,
        string artistText,
        string metaLine,
        BridgeRawReleaseEdit? edit,
        BridgeMetadataSource? source,
        string? actionLabel,
        Action? onAction,
        bool includeSelectedValues)
    {
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions($"{CoverSize},*,Auto"),
            ColumnSpacing = 14,
        };

        var cover = CoverTile(includeSelectedValues);
        Grid.SetColumn(cover, 0);
        grid.Children.Add(cover);

        var summary = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Top };
        var title = new TextBlock
        {
            Text = titleText,
            FontSize = 16,
            FontWeight = FontWeight.SemiBold,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        title[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextPrimaryBrush");
        summary.Children.Add(title);
        if (artistText.Length > 0)
        {
            summary.Children.Add(ImportPaneUi.Cell(artistText, secondary: true));
        }
        var facts = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        facts.Children.Add(ImportPaneUi.Cell(metaLine, secondary: true));
        if (source is { } pickedSource)
        {
            facts.Children.Add(SourceChip(pickedSource));
        }
        summary.Children.Add(facts);
        Grid.SetColumn(summary, 1);
        grid.Children.Add(summary);

        if (actionLabel is not null && onAction is not null)
        {
            var action = ActionButton(actionLabel, onAction);
            action.VerticalAlignment = VerticalAlignment.Top;
            Grid.SetColumn(action, 2);
            grid.Children.Add(action);
        }

        var body = new StackPanel { Spacing = 12, Children = { grid } };
        if (edit is not null)
        {
            body.Children.Add(new Expander
            {
                Header = Loc.Chrome("import.pane.details"),
                FontSize = 12,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                Content = ReleaseFields(edit),
            });
        }
        if (includeSelectedValues && CommitRow is not null)
        {
            body.Children.Add(CommitRow);
        }
        var card = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(12),
            BorderThickness = new Thickness(1),
            Child = body,
        };
        card[!Border.BorderBrushProperty] =
            new DynamicResourceExtension("BaeHairlineBrush");
        return card;
    }

    internal const double CoverSize = 160;

    private Control CoverTile(bool includeSelectedValues)
    {
        var image = new Image
        {
            Width = CoverSize,
            Height = CoverSize,
            Stretch = Stretch.UniformToFill,
        };
        if (includeSelectedValues)
        {
            LoadCover?.Invoke(image);
        }
        var tile = new Border
        {
            Width = CoverSize,
            Height = CoverSize,
            CornerRadius = new CornerRadius(6),
            ClipToBounds = true,
            Child = image,
            VerticalAlignment = VerticalAlignment.Top,
        };
        tile[!Border.BackgroundProperty] =
            new DynamicResourceExtension("BaeElevatedBrush");
        if (!includeSelectedValues || !HasCoverOptions)
        {
            return tile;
        }
        var button = new Button
        {
            Content = tile,
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            Background = Brushes.Transparent,
            Cursor = new Avalonia.Input.Cursor(
                Avalonia.Input.StandardCursorType.Hand),
            VerticalAlignment = VerticalAlignment.Top,
        };
        ToolTip.SetTip(button, Loc.Chrome("cover.change_title"));
        button.Click += (_, _) => OnEditCover();
        return button;
    }

    private static Control SourceChip(BridgeMetadataSource source)
    {
        var text = new TextBlock
        {
            Text = BaeBridgeMethods.BridgeMetadataSourceName(source),
            FontSize = 10.5,
            FontWeight = FontWeight.Medium,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        var chip = new Border
        {
            CornerRadius = new CornerRadius(999),
            Padding = new Thickness(5, 1),
            Child = text,
            VerticalAlignment = VerticalAlignment.Center,
        };
        chip[!Border.BackgroundProperty] =
            new DynamicResourceExtension("BaeElevatedBrush");
        return chip;
    }

    private Control ReleaseFields(BridgeRawReleaseEdit edit)
    {
        var column = new StackPanel { Spacing = 8, Margin = new Thickness(0, 8, 0, 0) };
        var album = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*"),
            ColumnSpacing = 8,
        };
        Add(album, 0, 0, "edit.field.album_title", edit.AlbumTitle,
            BridgeCandidateEditField.AlbumTitle);
        var artists = new ArtistAssignmentsField(
            edit.AlbumArtistAssignments,
            Library,
            OnEditArtists);
        album.Children.Add(new StackPanel
        {
            Spacing = 4,
            Children =
            {
                DialogUi.SectionLabel(Loc.Chrome("edit.field.album_artists")),
                artists,
            },
        }.WithGridColumn(1));
        column.Children.Add(album);

        var pressing = edit.Pressing;
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*,*"),
            RowDefinitions = new RowDefinitions("Auto,Auto"),
            ColumnSpacing = 8,
            RowSpacing = 6,
        };
        Add(grid, 0, 0, "edit.field.year", pressing.Year, BridgeCandidateEditField.Year);
        Add(grid, 1, 0, "edit.field.format", pressing.Format, BridgeCandidateEditField.Format);
        Add(grid, 2, 0, "edit.field.label", pressing.Label, BridgeCandidateEditField.Label);
        Add(grid, 0, 1, "edit.field.country", pressing.Country, BridgeCandidateEditField.Country);
        Add(grid, 1, 1, "edit.field.catalog_number", pressing.CatalogNumber, BridgeCandidateEditField.CatalogNumber);
        Add(grid, 2, 1, "edit.field.barcode", pressing.Barcode, BridgeCandidateEditField.Barcode);
        column.Children.Add(grid);
        return column;
    }

    private void Add(
        Grid grid,
        int column,
        int row,
        string labelKey,
        string value,
        BridgeCandidateEditField field)
    {
        var control = DialogUi.Field(Loc.Chrome(labelKey), out var box);
        box.FontSize = 12;
        box.Commits(value, typed => OnEditField(field, typed));
        Grid.SetColumn(control, column);
        Grid.SetRow(control, row);
        grid.Children.Add(control);
    }
}
