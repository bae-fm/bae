using System.Collections.Generic;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The album / pressing / track edit form shared by the metadata editor and the
// import confirmation step. Builds the album-title / artists / pressing fields plus
// the per-track table, seeds them from a raw edit, and reads the typed values back
// into the same shape. It owns no commit policy — the caller decides whether the
// read-back goes to apply-edit or to import-candidate — so both screens build the
// identical form. Shaping and validation happen in core; this only moves strings.
internal sealed class ReleaseEditForm
{
    // The panel to host inside a dialog's ScrollViewer.
    internal StackPanel Panel { get; }

    // In-form error line (salmon, hidden until set). Both callers show
    // validation / commit failures here rather than on the occluded window banner.
    internal TextBlock ErrorText { get; }

    private readonly TextBox _titleBox;
    private readonly ArtistAssignmentsField _artistField;
    private readonly TextBox _yearBox;
    private readonly TextBox _formatBox;
    private readonly TextBox _labelBox;
    private readonly TextBox _catalogBox;
    private readonly TextBox _countryBox;
    private readonly TextBox _barcodeBox;

    private readonly List<(BridgeRawTrackEdit Track, TextBox Title, ArtistAssignmentsField Artists, TextBox Side, TextBox Number)> _trackBoxes = new();
    private readonly List<Grid> _trackRows = new();
    private readonly LibraryService _library;

    private BridgeRawReleaseEdit _edit;

    internal ReleaseEditForm(
        BridgeRawReleaseEdit seed,
        double width,
        LibraryService library)
    {
        _library = library;
        _edit = seed;
        ErrorText = DialogUi.Danger();

        Panel = new StackPanel { Spacing = 8, Width = width };
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.album_title"), out _titleBox));
        _artistField = new ArtistAssignmentsField(
            seed.AlbumArtistAssignments,
            library,
            _ => { });
        Panel.Children.Add(LabeledControl(
            Loc.Chrome("edit.field.album_artists"),
            _artistField));
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.year"), out _yearBox));
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.format"), out _formatBox));
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.label"), out _labelBox));
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.catalog_number"), out _catalogBox));
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.country"), out _countryBox));
        Panel.Children.Add(DialogUi.Field(Loc.Chrome("edit.field.barcode"), out _barcodeBox));

        var tracksHeader = new TextBlock
        {
            Text = Loc.Chrome("edit.tracks.header"),
            FontWeight = FontWeight.SemiBold,
            Margin = new Thickness(0, 8, 0, 0),
        };
        tracksHeader[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        Panel.Children.Add(tracksHeader);

        var columnHeader = TrackGrid();
        HeaderCell(columnHeader, Loc.Chrome("edit.tracks.col_title"), 0);
        HeaderCell(columnHeader, Loc.Chrome("edit.tracks.col_artist"), 1);
        HeaderCell(columnHeader, Loc.Chrome("edit.tracks.col_side"), 2);
        HeaderCell(columnHeader, Loc.Chrome("edit.tracks.col_number"), 3);
        Panel.Children.Add(columnHeader);

        // ErrorText terminates the panel; track rows are inserted just before it so
        // reseeding rebuilds them without disturbing the album fields, the header,
        // or the error line.
        Panel.Children.Add(ErrorText);

        Seed(seed);
    }

    private static Grid TrackGrid() => new()
    {
        ColumnDefinitions = new ColumnDefinitions("*,*,56,56"),
        ColumnSpacing = 6,
    };

    private static void HeaderCell(Grid header, string text, int column)
    {
        var cell = new TextBlock { Text = text, FontSize = 12 };
        cell[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        Grid.SetColumn(cell, column);
        header.Children.Add(cell);
    }

    // Populate the album/pressing fields and rebuild the track table from a freshly
    // loaded edit, replacing the bound edit. Used for the initial seed and for reset
    // to source.
    internal void Seed(BridgeRawReleaseEdit edit)
    {
        _edit = edit;
        _titleBox.Text = edit.AlbumTitle;
        _artistField.SetAssignments(edit.AlbumArtistAssignments);
        _yearBox.Text = edit.Pressing.Year;
        _formatBox.Text = edit.Pressing.Format;
        _labelBox.Text = edit.Pressing.Label;
        _catalogBox.Text = edit.Pressing.CatalogNumber;
        _countryBox.Text = edit.Pressing.Country;
        _barcodeBox.Text = edit.Pressing.Barcode;

        foreach (var oldRow in _trackRows)
        {
            Panel.Children.Remove(oldRow);
        }
        _trackRows.Clear();
        _trackBoxes.Clear();

        var errorIndex = Panel.Children.IndexOf(ErrorText);
        foreach (var track in edit.Tracks)
        {
            var row = TrackGrid();
            var titleCell = new TextBox { Text = track.Title };
            var explicitArtists = track.ArtistAssignments as BridgeTrackArtistAssignments.Explicit;
            var artistCell = new ArtistAssignmentsField(
                explicitArtists?.Assignments ?? Array.Empty<BridgeArtistAssignment>(),
                _library,
                _ => { },
                inheritsAlbumArtists: track.ArtistAssignments is BridgeTrackArtistAssignments.AlbumArtists,
                onUseAlbumArtists: () => { });
            var sideCell = new TextBox { Text = track.Side.ToString() };
            var numberCell = new TextBox { Text = track.TrackNumber?.ToString() ?? string.Empty };
            Grid.SetColumn(titleCell, 0);
            Grid.SetColumn(artistCell, 1);
            Grid.SetColumn(sideCell, 2);
            Grid.SetColumn(numberCell, 3);
            row.Children.Add(titleCell);
            row.Children.Add(artistCell);
            row.Children.Add(sideCell);
            row.Children.Add(numberCell);
            Panel.Children.Insert(errorIndex, row);
            errorIndex++;
            _trackRows.Add(row);
            _trackBoxes.Add((track, titleCell, artistCell, sideCell, numberCell));
        }
    }

    // Flush the typed values into the raw edit shape for the caller to commit.
    internal BridgeRawReleaseEdit ReadBack()
    {
        var tracks = new List<BridgeRawTrackEdit>();
        foreach (var (track, titleCell, artistCell, sideCell, numberCell) in _trackBoxes)
        {
            // Keep the seeded side if the field was cleared/garbled (side is
            // required); a blank track number means "no number".
            var side = int.TryParse(sideCell.Text, out var parsedSide) ? parsedSide : track.Side;
            // The row's audio binding is carried through untouched: it is not
            // a form field, and dropping it here would unpair a track the user
            // had already paired.
            tracks.Add(new BridgeRawTrackEdit(
                track.Id,
                titleCell.Text ?? string.Empty,
                artistCell.InheritsAlbumArtists
                    ? new BridgeTrackArtistAssignments.AlbumArtists()
                    : new BridgeTrackArtistAssignments.Explicit(
                        artistCell.Assignments.ToArray()),
                side,
                int.TryParse(numberCell.Text, out var number) ? number : null,
                track.File));
        }

        _edit = new BridgeRawReleaseEdit(
            _titleBox.Text ?? string.Empty,
            _artistField.Assignments.ToArray(),
            new BridgeRawPressingEdit(
                _yearBox.Text ?? string.Empty,
                _formatBox.Text ?? string.Empty,
                _labelBox.Text ?? string.Empty,
                _catalogBox.Text ?? string.Empty,
                _countryBox.Text ?? string.Empty,
                _barcodeBox.Text ?? string.Empty),
            tracks.ToArray());
        return _edit;
    }

    private static Control LabeledControl(string label, Control control) =>
        new StackPanel
        {
            Spacing = 4,
            Children =
            {
                new TextBlock { Text = label, FontSize = 12.5 },
                control,
            },
        };
}
