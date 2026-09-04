using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>One source a draft's metadata claims, as the card names it: the
/// service's own name, linking to that source's page for the release.</summary>
internal readonly record struct ProvenanceChip(string Label, Uri? Link);

/// <summary>The editable draft or one temporary source browser occupying the
/// metadata slot.</summary>
internal sealed class ImportMetadataSourceSection
{
    internal required ImportMetadataPresentation Presentation { get; init; }
    internal required bool DraftIsBlank { get; init; }
    internal required string Title { get; init; }
    internal required BridgeRawReleaseEdit? Edit { get; init; }
    internal required string MetaLine { get; init; }
    internal required string SourceAudioLine { get; init; }
    /// <summary>One chip per source the draft's provenance claims, in the
    /// order they are shown: the release the draft was read from, then each
    /// partner the pick carried. Empty when there is no provenance.</summary>
    internal required IReadOnlyList<ProvenanceChip> ProvenanceChips { get; init; }
    internal required bool IsReading { get; init; }
    internal required BridgeReleaseUserEdit? FileTagsPreview { get; init; }
    internal required string FileTagsMetaLine { get; init; }
    internal required string? FileTagsError { get; init; }
    internal required Control? LookupOptions { get; init; }
    internal required Action<Image>? LoadCover { get; init; }
    internal required bool HasCoverOptions { get; init; }
    internal required Control? CommitRow { get; init; }
    internal required LibraryService Library { get; init; }
    internal required Action<ImportMetadataPresentation> OnPresent { get; init; }
    internal required Action OnReadFileTags { get; init; }
    internal required Action OnUseFileTags { get; init; }
    internal required Action OnClearMetadata { get; init; }
    internal required Action OnEditCover { get; init; }
    internal required Action<BridgeCoverSelection> OnSelectCover { get; init; }
    internal required Action<BridgeCandidateEditField, string> OnEditField { get; init; }
    internal required Action<IReadOnlyList<BridgeArtistAssignment>> OnEditArtists { get; init; }

    internal Control Build()
    {
        return Presentation switch
        {
            ImportMetadataPresentation.Draft => DraftContent(),
            ImportMetadataPresentation.FindOnline => FindOnlineContent(),
            ImportMetadataPresentation.FileTags => FileTagsContent(),
            _ => throw new ArgumentOutOfRangeException(
                nameof(Presentation), Presentation, "Unknown metadata presentation"),
        };
    }

    private Control FindOnlineContent()
    {
        var column = new StackPanel { Spacing = 8 };
        column.Children.Add(BrowserHeader(Loc.Chrome("import.metadata.find_online")));
        if (LookupOptions is not null)
        {
            column.Children.Add(LookupOptions);
        }
        return column;
    }

    private Control DraftContent()
    {
        if (Edit is null)
        {
            return new Spinner { Width = 16, Height = 16 };
        }
        if (DraftIsBlank)
        {
            return BlankDraftCard(Edit);
        }
        return Card(
            Title,
            ArtistAssignmentDisplay.Join(Edit.AlbumArtistAssignments),
            MetaLine,
            SourceAudioLine,
            Edit,
            ProvenanceChips,
            SourceActions(),
            ClearMetadataAction(),
            includeSelectedValues: true);
    }

    private Control BlankDraftCard(BridgeRawReleaseEdit edit)
    {
        var layout = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions($"{CoverSize},*"),
            ColumnSpacing = 14,
        };
        var cover = CoverTile(includeSelectedValues: true);
        Grid.SetColumn(cover, 0);
        layout.Children.Add(cover);

        var editor = new StackPanel { Spacing = 12 };
        editor.Children.Add(SourceActions());
        editor.Children.Add(ReleaseFields(edit));
        Grid.SetColumn(editor, 1);
        layout.Children.Add(editor);

        var body = new StackPanel { Spacing = 12, Children = { layout } };
        if (CommitRow is not null)
        {
            body.Children.Add(CommitRow);
        }
        return CardBorder(body);
    }

    private Control SourceActions()
    {
        var actions = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
        };
        actions.Children.Add(ActionButton(
            Loc.Chrome("import.metadata.find_online_ellipsis"),
            () => OnPresent(ImportMetadataPresentation.FindOnline)));
        actions.Children.Add(ActionButton(
            Loc.Core("ui.import.metadata.file_tags") + "…",
            () => OnPresent(ImportMetadataPresentation.FileTags)));
        return actions;
    }

    private Control ClearMetadataAction()
    {
        var clear = ActionButton(
            Loc.Chrome("import.metadata.clear"),
            OnClearMetadata);
        clear.HorizontalAlignment = HorizontalAlignment.Right;
        clear[!Button.ForegroundProperty] =
            new DynamicResourceExtension("BaeDangerBrush");
        return clear;
    }

    private Control FileTagsContent()
    {
        var column = new StackPanel { Spacing = 8 };
        column.Children.Add(BrowserHeader(Loc.Core("ui.import.metadata.file_tags")));
        if (FileTagsPreview is { } preview)
        {
            column.Children.Add(Card(
                preview.AlbumTitle,
                ArtistAssignmentDisplay.Join(preview.AlbumArtistAssignments),
                FileTagsMetaLine,
                SourceAudioLine,
                edit: null,
                provenanceChips: [],
                actionControl: ActionButton(
                    Loc.Chrome("import.metadata.apply"),
                    OnUseFileTags),
                destructiveAction: null,
                includeSelectedValues: false));
            return column;
        }
        if (IsReading)
        {
            column.Children.Add(new Spinner
            {
                Width = 16,
                Height = 16,
                HorizontalAlignment = HorizontalAlignment.Center,
            });
            return column;
        }
        column.Children.Add(ActionButton(
            Loc.Chrome("import.metadata.try_again"),
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

    private Control BrowserHeader(string title)
    {
        var row = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto,*"),
        };
        var back = ActionButton(
            Loc.Chrome("action.back"),
            () => OnPresent(ImportMetadataPresentation.Draft));
        Grid.SetColumn(back, 0);
        back.HorizontalAlignment = HorizontalAlignment.Left;
        row.Children.Add(back);
        var heading = ImportPaneUi.Cell(title);
        Grid.SetColumn(heading, 1);
        row.Children.Add(heading);
        return row;
    }

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
        string sourceAudioLine,
        BridgeRawReleaseEdit? edit,
        IReadOnlyList<ProvenanceChip> provenanceChips,
        Control? actionControl,
        Control? destructiveAction,
        bool includeSelectedValues)
    {
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions($"{CoverSize},*"),
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
        foreach (var chip in provenanceChips)
        {
            facts.Children.Add(SourceChip(chip.Label, chip.Link));
        }
        summary.Children.Add(facts);
        summary.Children.Add(ImportPaneUi.Cell(sourceAudioLine, secondary: true));
        var summaryRow = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            ColumnSpacing = 14,
        };
        summaryRow.Children.Add(summary);

        if (actionControl is not null)
        {
            actionControl.VerticalAlignment = VerticalAlignment.Top;
            Grid.SetColumn(actionControl, 1);
            summaryRow.Children.Add(actionControl);
        }

        var metadata = new StackPanel { Spacing = 12 };
        metadata.Children.Add(summaryRow);
        if (edit is not null)
        {
            metadata.Children.Add(new Expander
            {
                Header = Loc.Chrome("import.pane.details"),
                FontSize = 12,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                Content = ReleaseFields(edit),
            });
        }
        if (destructiveAction is not null)
        {
            metadata.Children.Add(destructiveAction);
        }
        Grid.SetColumn(metadata, 1);
        grid.Children.Add(metadata);

        var body = new StackPanel { Spacing = 12, Children = { grid } };
        if (includeSelectedValues && CommitRow is not null)
        {
            body.Children.Add(CommitRow);
        }
        return CardBorder(body);
    }

    private static Border CardBorder(Control body)
    {
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
        if (includeSelectedValues)
        {
            EnableCoverDrop(tile);
        }
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

    private void EnableCoverDrop(Border tile)
    {
        DragDrop.SetAllowDrop(tile, true);
        tile.AddHandler(DragDrop.DragOverEvent, (_, e) =>
        {
            if (e.DataTransfer.Contains(ImportMappingGallery.CoverDragFormat))
            {
                e.DragEffects = DragDropEffects.Copy;
                e.Handled = true;
                tile.BorderThickness = new Thickness(3);
                tile[!Border.BorderBrushProperty] =
                    new DynamicResourceExtension("BaeAccentBrush");
            }
        });
        tile.AddHandler(DragDrop.DragLeaveEvent, (_, _) =>
            tile.BorderThickness = new Thickness(0));
        tile.AddHandler(DragDrop.DropEvent, (_, e) =>
        {
            tile.BorderThickness = new Thickness(0);
            if (e.DataTransfer.TryGetValue(ImportMappingGallery.CoverDragFormat)
                is not string fileId)
            {
                return;
            }
            e.Handled = true;
            OnSelectCover(new BridgeCoverSelection.ReleaseImage(fileId));
        });
    }

    private static Control SourceChip(string label, Uri? uri)
    {
        var text = new TextBlock
        {
            Text = label,
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
        if (uri is null)
        {
            return chip;
        }
        var button = new Button
        {
            Content = chip,
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            Background = Brushes.Transparent,
            Cursor = new Avalonia.Input.Cursor(
                Avalonia.Input.StandardCursorType.Hand),
        };
        button.Click += async (_, _) =>
        {
            var launcher = TopLevel.GetTopLevel(button)?.Launcher;
            if (launcher is null)
            {
                BaeDiagnostics.Logger.Warning(
                    $"Open metadata source failed: no launcher for {uri.Host}");
                return;
            }
            try
            {
                if (!await launcher.LaunchUriAsync(uri))
                {
                    BaeDiagnostics.Logger.Warning(
                        $"Open metadata source failed: launcher rejected {uri.Host}");
                }
            }
            catch (Exception exception)
            {
                BaeDiagnostics.Logger.Warning(
                    $"Open metadata source failed: {exception.Message}");
            }
        };
        return button;
    }

    private Control ReleaseFields(BridgeRawReleaseEdit edit)
    {
        var column = new StackPanel { Spacing = 8, Margin = new Thickness(0, 8, 0, 0) };
        var album = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*,Auto"),
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
        Add(album, 2, 0, Loc.Chrome("edit.field.year"), edit.AlbumYear,
            BridgeCandidateEditField.AlbumYear);
        column.Children.Add(album);

        var pressing = edit.Pressing;
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*,*"),
            RowDefinitions = new RowDefinitions("Auto,Auto"),
            ColumnSpacing = 8,
            RowSpacing = 6,
        };
        Add(grid, 0, 0, Loc.Chrome("edit.field.year"), pressing.Year, BridgeCandidateEditField.PressingYear);
        Add(grid, 1, 0, Loc.Core("core.release.media"), pressing.Format, BridgeCandidateEditField.Format);
        Add(grid, 2, 0, Loc.Chrome("edit.field.label"), pressing.Label, BridgeCandidateEditField.Label);
        Add(grid, 0, 1, Loc.Chrome("edit.field.country"), pressing.Country, BridgeCandidateEditField.Country);
        Add(grid, 1, 1, Loc.Chrome("edit.field.catalog_number"), pressing.CatalogNumber, BridgeCandidateEditField.CatalogNumber);
        Add(grid, 2, 1, Loc.Chrome("edit.field.barcode"), pressing.Barcode, BridgeCandidateEditField.Barcode);
        column.Children.Add(grid);
        return column;
    }

    private void Add(
        Grid grid,
        int column,
        int row,
        string label,
        string value,
        BridgeCandidateEditField field)
    {
        var control = DialogUi.Field(label, out var box);
        box.FontSize = 12;
        box.Commits(value, typed => OnEditField(field, typed));
        Grid.SetColumn(control, column);
        Grid.SetRow(control, row);
        grid.Children.Add(control);
    }
}
