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
/// Section 1 of the mapping pane: what the folder is being read as.
///
/// The release ⇄ Unknown control sits here, always visible, because it is the
/// question this section answers — not a link inside the search. Both sides
/// leave a mapping table to work in: a release names the tracklist, and Unknown
/// reads it off the folder's own files.
/// </summary>
internal sealed class ImportIdentitySection
{
    /// <summary>Which side of the control is in force.</summary>
    internal required ImportIdentity Identity { get; init; }

    /// <summary>The folder on disk — its own line, not the release title: what
    /// the folder is called and what the release is are different facts, and
    /// the card leads with the release.</summary>
    internal required string FolderName { get; init; }

    /// <summary>The folder's audio shape ("FLAC", "CUE+FLAC"), shown beside its
    /// name.</summary>
    internal required string FormatLabel { get; init; }

    /// <summary>Whether anything has been settled for this folder — a release
    /// picked or its own tags read. Until then there is no release card to
    /// show: the search editor below offers the matches, and the folder line
    /// already says what this is.</summary>
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

    /// <summary>What identified the picked release, drawn as the badge the
    /// pressing rows carry. Null before a pick, for a folder read as its own
    /// tags, and for a release a typed search found — a badge there would claim
    /// evidence about the disc there is none of.</summary>
    internal required BridgeClaimEvidence? Evidence { get; init; }

    /// <summary>Whether a release has been picked — what the change control
    /// reads as.</summary>
    internal required bool HasPick { get; init; }

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
        column.Children.Add(FolderLine());
        column.Children.Add(IdentityPicker());
        if (HasSettled)
        {
            column.Children.Add(Card());
        }
        return column;
    }

    /// <summary>The folder itself: name in mono, its audio shape beside it.
    /// Always present, whatever the release side is showing.</summary>
    private Control FolderLine()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        var name = new TextBlock
        {
            Text = FolderName,
            FontSize = 11.5,
            FontFamily = new FontFamily("monospace"),
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        row.Children.Add(name);
        if (FormatLabel.Length > 0)
        {
            var format = new TextBlock
            {
                Text = FormatLabel,
                FontSize = 11,
                VerticalAlignment = VerticalAlignment.Center,
            };
            format[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            row.Children.Add(format);
        }
        return row;
    }

    /// <summary>The one control that switches sides. Picking Unknown reads the
    /// folder's own tags; picking Release re-picks the release the candidate
    /// already holds, or opens the search when it holds none.</summary>
    private Control IdentityPicker()
    {
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        row.Children.Add(Segment(Loc.Core("ui.import.identity.release"), ImportIdentity.Release));
        row.Children.Add(Segment(Loc.Core("ui.import.identity.unknown"), ImportIdentity.Unknown));
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
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("80,*,Auto"), ColumnSpacing = 14 };

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
        summary.Children.Add(ImportPaneUi.Cell(MetaLine, secondary: true));
        if (EvidenceBadge() is { } badge)
        {
            summary.Children.Add(badge);
        }
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
                Header = Loc.Chrome("import.pane.release_details"),
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

    private Control CoverTile()
    {
        var image = new Image { Width = 80, Height = 80, Stretch = Stretch.UniformToFill };
        LoadCover?.Invoke(image);
        var tile = new Border
        {
            Width = 80,
            Height = 80,
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

    /// <summary>What turned this release up, in the same chip the pressing rows
    /// carry. A typed search is not evidence about the disc, so it draws
    /// nothing.</summary>
    private Control? EvidenceBadge() => Evidence switch
    {
        BridgeClaimEvidence.DiscIdAlone or BridgeClaimEvidence.DiscIdShared =>
            SignalBadgeRow.Chip(Loc.Chrome("signal.kind.disc_id")),
        BridgeClaimEvidence.Barcode => SignalBadgeRow.Chip(Loc.Chrome("signal.kind.barcode")),
        _ => null,
    };

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
        Add(grid, 0, 1, "edit.field.catalog_number", pressing.CatalogNumber, BridgeCandidateEditField.CatalogNumber);
        Add(grid, 1, 1, "edit.field.country", pressing.Country, BridgeCandidateEditField.Country);
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
