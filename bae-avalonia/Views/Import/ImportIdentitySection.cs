using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>What a folder is being read as.</summary>
internal enum ImportIdentity
{
    /// <summary>A release picked from a metadata source names the
    /// tracklist.</summary>
    Release,

    /// <summary>The folder's own files do — their embedded tags, or the track
    /// sheets they come with.</summary>
    Unknown,
}

/// <summary>
/// Section 1 of the mapping pane: where the folder's metadata comes from.
///
/// The lookup ⇄ file-tags control sits here, always visible, because it is the
/// question this section answers — not a link inside the search. Both sides
/// leave a mapping table to work in: a looked-up release names the tracklist,
/// and file tags read it off the folder's own files.
/// </summary>
internal sealed class ImportIdentitySection
{
    /// <summary>Which side of the control is in force.</summary>
    internal required ImportIdentity Identity { get; init; }

    /// <summary>Whether anything has been settled for this folder — a release
    /// picked or its own tags read. Until then there is no release card to
    /// show: the search editor below offers the matches, and the folder line
    /// at the top of the pane already says what this is.</summary>
    internal required bool HasSettled { get; init; }

    /// <summary>The album line the card leads with: what is being edited once
    /// there is something, and the folder's own name before that.</summary>
    internal required string Title { get; init; }

    /// <summary>The metadata form: the pick's own values with whatever has been
    /// typed over them. Null while nothing is picked.</summary>
    internal required BridgeRawReleaseEdit? Edit { get; init; }

    /// <summary>"CD · 1996 · 9 tracks", from what is being edited rather than
    /// what was fetched.</summary>
    internal required string MetaLine { get; init; }

    /// <summary>Whether a release has been picked — what the change control
    /// reads as.</summary>
    internal required bool HasPick { get; init; }

    /// <summary>Which service the picked release came from. Null before a pick
    /// and for a folder read as its own tags — there is no service behind
    /// either.</summary>
    internal required BridgeMetadataSource? PickedSource { get; init; }

    /// <summary>Whether a read is in flight. The controls that start one read as
    /// pending rather than the section being replaced by a placeholder.</summary>
    internal required bool IsReading { get; init; }

    /// <summary>Paint the cover tile. Null while there is no cover to
    /// show.</summary>
    internal required Action<Image>? LoadCover { get; init; }

    internal required bool HasCoverOptions { get; init; }

    /// <summary>The card's commit row — what is unanswered, storage, and the
    /// Import action. Null while there is nothing to commit.</summary>
    internal required Control? CommitRow { get; init; }

    internal required Action<ImportIdentity> OnSetIdentity { get; init; }

    internal required Action OnFindRelease { get; init; }

    internal required Action OnEditCover { get; init; }

    /// <summary>Store one album-level field as the user left it.</summary>
    internal required Action<BridgeCandidateEditField, string> OnEditField { get; init; }

    internal Control Build()
    {
        var column = new StackPanel { Spacing = 10 };
        column.Children.Add(IdentityPicker());
        if (HasSettled)
        {
            column.Children.Add(Card());
        }
        return column;
    }

    /// <summary>The one control that switches sides. Picking file tags reads
    /// the folder's own; picking lookup re-picks the release the candidate
    /// already holds, or opens the search when it holds none.</summary>
    private Control IdentityPicker()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        row.Children.Add(Segment(Loc.Core("ui.import.metadata.lookup"), ImportIdentity.Release));
        row.Children.Add(Segment(Loc.Core("ui.import.metadata.file_tags"), ImportIdentity.Unknown));
        return row;
    }

    private Control Segment(string label, ImportIdentity identity)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = !IsReading;
        if (Identity == identity)
        {
            button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
            button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        }
        button.Click += (_, _) => OnSetIdentity(identity);
        return button;
    }

    // The card: the cover, what the release is, the claim this import records,
    // and the control that opens the search to change which release it is.
    private Control Card()
    {
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions($"{CoverSize},*,Auto"),
            ColumnSpacing = 14,
        };

        var cover = CoverTile();
        Grid.SetColumn(cover, 0);
        grid.Children.Add(cover);

        var summary = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Top };
        var title = new TextBlock
        {
            Text = Title,
            FontSize = 16,
            FontWeight = FontWeight.SemiBold,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        summary.Children.Add(title);
        summary.Children.Add(ImportPaneUi.Cell(Edit?.AlbumArtistText ?? string.Empty, secondary: true));
        var facts = new StackPanel
        {
            Orientation = Avalonia.Layout.Orientation.Horizontal,
            Spacing = 6,
        };
        facts.Children.Add(ImportPaneUi.Cell(MetaLine, secondary: true));
        if (SourceChip() is { } source)
        {
            facts.Children.Add(source);
        }
        summary.Children.Add(facts);
        Grid.SetColumn(summary, 1);
        grid.Children.Add(summary);

        var change = ImportPaneUi.RowButton(Loc.Core(
            HasPick ? "ui.import.header.change_release" : "ui.import.header.find_release"));
        change.IsEnabled = !IsReading;
        change.VerticalAlignment = VerticalAlignment.Top;
        change.Click += (_, _) => OnFindRelease();
        Grid.SetColumn(change, 2);
        grid.Children.Add(change);

        var body = new StackPanel { Spacing = 12 };
        body.Children.Add(grid);
        // The card's own fold, at its foot: the card above states what these
        // fields add up to, and this is where a wrong year or a missing catalog
        // number gets fixed before it is written. The whole header line is the
        // control — a caret is a target the width of a glyph.
        if (Edit is not null)
        {
            body.Children.Add(new Expander
            {
                Header = Loc.Chrome("import.pane.details"),
                FontSize = 12,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                Content = ReleaseFields(),
            });
        }
        if (CommitRow is not null)
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
        card[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        return card;
    }

    /// <summary>The cover the card leads with. Big enough to read the artwork
    /// as artwork — at a thumbnail's size it was an icon beside the title, and
    /// the cover is the thing being confirmed.</summary>
    internal const double CoverSize = 160;

    private Control CoverTile()
    {
        var image = new Image
        {
            Width = CoverSize,
            Height = CoverSize,
            Stretch = Stretch.UniformToFill,
        };
        LoadCover?.Invoke(image);
        var tile = new Border
        {
            Width = CoverSize,
            Height = CoverSize,
            CornerRadius = new CornerRadius(6),
            ClipToBounds = true,
            Child = image,
            VerticalAlignment = VerticalAlignment.Top,
        };
        tile[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        if (!HasCoverOptions)
        {
            return tile;
        }
        var button = new Button
        {
            Content = tile,
            Padding = new Thickness(0),
            BorderThickness = new Thickness(0),
            Background = Brushes.Transparent,
            Cursor = new Avalonia.Input.Cursor(Avalonia.Input.StandardCursorType.Hand),
            VerticalAlignment = VerticalAlignment.Top,
        };
        ToolTip.SetTip(button, Loc.Chrome("cover.change_title"));
        button.Click += (_, _) => OnEditCover();
        return button;
    }

    /// <summary>Where the picked release came from. The service's own name,
    /// never a code and never translated — it is a brand, and the name is what
    /// the person recognises.</summary>
    private Control? SourceChip()
    {
        if (PickedSource is not { } source)
        {
            return null;
        }
        var text = new TextBlock
        {
            Text = BaeBridgeMethods.BridgeMetadataSourceName(source),
            FontSize = 10.5,
            FontWeight = FontWeight.Medium,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var chip = new Border
        {
            CornerRadius = new CornerRadius(999),
            Padding = new Thickness(5, 1),
            Child = text,
            VerticalAlignment = VerticalAlignment.Center,
        };
        chip[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        return chip;
    }

    // The release's own fields: the album line the card states, and the pressing
    // this import records. Each field is a row under the candidate, so leaving
    // one writes it and the pane's next value redraws.
    private Control ReleaseFields()
    {
        var edit = Edit!;
        var column = new StackPanel { Spacing = 8, Margin = new Thickness(0, 8, 0, 0) };

        var album = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*"), ColumnSpacing = 8 };
        Add(album, 0, 0, "edit.field.album_title", edit.AlbumTitle, BridgeCandidateEditField.AlbumTitle);
        Add(album, 1, 0, "edit.field.album_artists", edit.AlbumArtistText, BridgeCandidateEditField.AlbumArtistText);
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
        Grid grid, int column, int row, string labelKey, string value, BridgeCandidateEditField field)
    {
        var control = DialogUi.Field(Loc.Chrome(labelKey), out var box);
        box.FontSize = 12;
        box.Commits(value, typed => OnEditField(field, typed));
        Grid.SetColumn(control, column);
        Grid.SetRow(control, row);
        grid.Children.Add(control);
    }
}
