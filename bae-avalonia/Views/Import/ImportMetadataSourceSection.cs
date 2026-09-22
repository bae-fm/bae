using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

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
    /// <summary>Every name the folder states, drawn under the audio facts.
    /// Empty until something has read it.</summary>
    /// <summary>What the rip databases said about the folder's audio, drawn
    /// under the names it states. <c>null</c> until something has read its
    /// log.</summary>
    /// <summary>Every catalog that describes the release the draft was read
    /// from, drawn last in the card under a rule of their own. Empty for a
    /// draft read from the files' own tags, or typed in.</summary>
    internal required IReadOnlyList<BridgeReleaseRecord> Records { get; init; }
    internal required bool IsReading { get; init; }
    internal required Control? LookupOptions { get; init; }
    internal required Action<Image>? LoadCover { get; init; }
    internal required bool HasCoverOptions { get; init; }
    internal required Control? CommitRow { get; init; }
    internal required LibraryService Library { get; init; }
    internal required Action<ImportMetadataPresentation> OnPresent { get; init; }
    /// <summary>Identify the candidate now: open the page its run reports on
    /// and ask core for a fresh run.</summary>
    internal required Action OnIdentify { get; init; }
    /// <summary>Open the same page on its typed search, starting nothing.</summary>
    internal required Action OnSearchForRelease { get; init; }
    internal required Action OnResetToFileMetadata { get; init; }
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
            SourceActions(),
            CandidateActions(),
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
        editor.Children.Add(ReleaseFields(edit));
        Grid.SetColumn(editor, 1);
        layout.Children.Add(editor);

        var body = new StackPanel { Spacing = 12, Children = { SourceActions(), layout } };
        if (CommitRow is not null)
        {
            body.Children.Add(CommitRow);
        }
        return CardBorder(body);
    }

    private Control SourceActions()
    {
        var actions = new WrapPanel { Orientation = Orientation.Horizontal };
        actions.Children.Add(ActionButton(
            Loc.Chrome("settings.import.identify_automatically"),
            OnIdentify));
        actions.Children.Add(ActionButton(
            Loc.Chrome("import.metadata.search_for_release"),
            OnSearchForRelease));
        foreach (var action in actions.Children)
        {
            action.Margin = new Thickness(0, 0, 6, 6);
        }
        return actions;
    }

    /// <summary>The two commands that rewrite the draft in place. Both are
    /// destructive — each replaces what the draft holds — and the pane asks
    /// before running either.</summary>
    private Control CandidateActions()
    {
        var actions = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        foreach (var (label, run) in new (string, Action)[]
        {
            (Loc.Chrome("import.metadata.reset_to_file_metadata"), OnResetToFileMetadata),
            (Loc.Chrome("import.metadata.clear"), OnClearMetadata),
        })
        {
            var button = ActionButton(label, run);
            button[!Button.ForegroundProperty] =
                new DynamicResourceExtension("BaeDangerBrush");
            actions.Children.Add(button);
        }
        return actions;
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
        summary.Children.Add(ImportPaneUi.Cell(metaLine, secondary: true));
        summary.Children.Add(ImportPaneUi.Cell(sourceAudioLine, secondary: true));
        var metadata = new StackPanel { Spacing = 12 };
        metadata.Children.Add(summary);
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

        var body = new StackPanel { Spacing = 12 };
        if (actionControl is not null)
        {
            body.Children.Add(actionControl);
        }
        body.Children.Add(grid);
        // The names the folder states and what the rip databases said: a
        // block of its own under the cover row, the full width of the card.
        // Which catalogs describe the release, last in the card under a rule
        // of their own.
        if (Records.Count > 0)
        {
            body.Children.Add(new Separator());
            body.Children.Add(ReleaseRecordsRow.Build(Records, ReleaseFactsScale.Pane));
        }
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

    internal const double CoverSize = 132;

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

        // One field per row: year, media, label, country, catalog, barcode.
        var pressing = edit.Pressing;
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*"),
            RowDefinitions = new RowDefinitions("Auto,Auto,Auto,Auto,Auto,Auto"),
            RowSpacing = 8,
        };
        Add(grid, 0, 0, Loc.Chrome("edit.field.year"), pressing.Year, BridgeCandidateEditField.PressingYear);
        Add(grid, 0, 1, Loc.Core("core.release.media"), pressing.Format, BridgeCandidateEditField.Format);
        Add(grid, 0, 2, Loc.Chrome("edit.field.label"), pressing.Label, BridgeCandidateEditField.Label);
        Add(grid, 0, 3, Loc.Chrome("edit.field.country"), pressing.Country, BridgeCandidateEditField.Country);
        Add(grid, 0, 4, Loc.Chrome("edit.field.catalog_number"), pressing.CatalogNumber, BridgeCandidateEditField.CatalogNumber);
        Add(grid, 0, 5, Loc.Chrome("edit.field.barcode"), pressing.Barcode, BridgeCandidateEditField.Barcode);
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
