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

    /// <summary>The album line the card leads with: what is being edited once
    /// there is something, and the folder's own name before that.</summary>
    internal required string Title { get; init; }

    /// <summary>The album title as the field holds it — blank until something is
    /// settled, where <see cref="Title"/> stands the folder's name in.</summary>
    internal required string AlbumTitle { get; init; }

    internal required string AlbumArtistText { get; init; }

    /// <summary>"CD · 1996 · 9 tracks", from what is being edited rather than
    /// what was fetched.</summary>
    internal required string MetaLine { get; init; }

    /// <summary>What this import claims to hold and where its metadata came
    /// from, as core derived it. Null before a pick, and for Unknown, which
    /// claims nothing.</summary>
    internal required BridgeClaimLine? Claim { get; init; }

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

    /// <summary>The pressing this import records, edited behind a disclosure
    /// alongside the album fields. Null until something has been settled for
    /// this folder and there is a release to edit at all.</summary>
    internal required BridgeRawPressingEdit? Pressing { get; init; }

    internal required Action<ImportIdentity> OnSetIdentity { get; init; }

    internal required Action OnFindRelease { get; init; }

    internal required Action OnEditCover { get; init; }

    internal required Action<string> OnAlbumTitle { get; init; }

    internal required Action<string> OnAlbumArtist { get; init; }

    internal required Action<BridgeRawPressingEdit> OnPressing { get; init; }

    internal Control Build()
    {
        var column = new StackPanel { Spacing = 10 };
        column.Children.Add(IdentityPicker());
        column.Children.Add(Card());
        if (Pressing is not null)
        {
            column.Children.Add(new Expander
            {
                Header = Loc.Chrome("import.pane.release_details"),
                FontSize = 12,
                HorizontalAlignment = HorizontalAlignment.Stretch,
                Content = ReleaseFields(),
            });
        }
        return column;
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
        summary.Children.Add(ImportPaneUi.Cell(AlbumArtistText, secondary: true));
        summary.Children.Add(ImportPaneUi.Cell(MetaLine, secondary: true));
        // Stated, never asked: bae-core derived the claim from the evidence that
        // identified the candidate, and picking a different release is what
        // moves it. An import that claims nothing has no source release to name.
        if (Claim is { } claim)
        {
            summary.Children.Add(ClaimLineView.Build(claim));
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

        var card = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(12),
            BorderThickness = new Thickness(1),
            Child = grid,
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

    // The release's own fields, folded away: the album line the card states, and
    // the pressing this import records. Behind a disclosure because the card
    // above already says what they add up to; this is where a wrong year or a
    // missing catalog number gets fixed before it is written.
    private Control ReleaseFields()
    {
        var column = new StackPanel { Spacing = 8, Margin = new Thickness(0, 8, 0, 0) };

        var album = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*"), ColumnSpacing = 8 };
        Add(album, 0, 0, "edit.field.album_title", AlbumTitle, OnAlbumTitle);
        Add(album, 1, 0, "edit.field.album_artists", AlbumArtistText, OnAlbumArtist);
        column.Children.Add(album);

        var pressing = Pressing!;
        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*,*"),
            RowDefinitions = new RowDefinitions("Auto,Auto"),
            ColumnSpacing = 8,
            RowSpacing = 6,
        };
        Add(grid, 0, 0, "edit.field.year", pressing.Year,
            value => OnPressing(pressing with { Year = value }));
        Add(grid, 1, 0, "edit.field.format", pressing.Format,
            value => OnPressing(pressing with { Format = value }));
        Add(grid, 2, 0, "edit.field.label", pressing.Label,
            value => OnPressing(pressing with { Label = value }));
        Add(grid, 0, 1, "edit.field.catalog_number", pressing.CatalogNumber,
            value => OnPressing(pressing with { CatalogNumber = value }));
        Add(grid, 1, 1, "edit.field.country", pressing.Country,
            value => OnPressing(pressing with { Country = value }));
        Add(grid, 2, 1, "edit.field.barcode", pressing.Barcode,
            value => OnPressing(pressing with { Barcode = value }));
        column.Children.Add(grid);
        return column;
    }

    private static void Add(Grid grid, int column, int row, string labelKey, string value, Action<string> write)
    {
        var field = DialogUi.Field(Loc.Chrome(labelKey), out var box);
        box.Text = value;
        box.FontSize = 12;
        box.TextChanged += (_, _) => write(box.Text ?? string.Empty);
        Grid.SetColumn(field, column);
        Grid.SetRow(field, row);
        grid.Children.Add(field);
    }
}
