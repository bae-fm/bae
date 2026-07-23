using System.Collections.Generic;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// The album/pressing/track edit form shared by the metadata editor and the
/// import confirmation step. Builds the album-title / artists / pressing
/// TextBoxes plus the per-track table, seeds them from a generated raw edit, and
/// reads the user's typed values back into the same generated shape. It owns no
/// commit policy — the caller decides whether the read-back goes to apply-edit
/// or import-candidate — so both screens build the identical form without
/// duplicating its layout.
/// </summary>
internal sealed class ReleaseEditForm
{
    /// <summary>The panel to host inside a dialog's <c>ScrollViewer</c>.</summary>
    internal StackPanel Panel { get; }

    /// <summary>
    /// In-form error line (salmon, hidden until set). Both callers show
    /// validation/commit failures here rather than on the occluded window banner.
    /// </summary>
    internal TextBlock ErrorText { get; }

    private readonly TextBox _titleBox = new() { Header = Loc.Chrome("edit.field.album_title") };
    private readonly TextBox _artistBox = new() { Header = Loc.Chrome("edit.field.album_artists") };
    private readonly TextBox _yearBox = new() { Header = Loc.Chrome("edit.field.year") };
    private readonly TextBox _formatBox = new() { Header = Loc.Chrome("edit.field.format") };
    private readonly TextBox _labelBox = new() { Header = Loc.Chrome("edit.field.label") };
    private readonly TextBox _catalogBox = new() { Header = Loc.Chrome("edit.field.catalog_number") };
    private readonly TextBox _countryBox = new() { Header = Loc.Chrome("edit.field.country") };
    private readonly TextBox _barcodeBox = new() { Header = Loc.Chrome("edit.field.barcode") };

    private readonly List<(BridgeRawTrackEdit Track, TextBox Title, TextBox Artist, TextBox Side, TextBox Number)> _trackBoxes = new();
    private readonly List<Grid> _trackRows = new();

    /// <summary>
    /// The edit currently bound to the form. Reseeding (e.g. reset to source)
    /// replaces it; <see cref="ReadBack"/> flushes the TextBoxes into it and
    /// returns it for the caller to pass to the bridge.
    /// </summary>
    private BridgeRawReleaseEdit _edit;

    /// <param name="seed">The edit to populate the form from.</param>
    /// <param name="width">Dialog content width (the two callers differ).</param>
    internal ReleaseEditForm(BridgeRawReleaseEdit seed, double width)
    {
        _edit = seed;

        ErrorText = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        Panel = new StackPanel { Spacing = 8, Width = width };
        Panel.Children.Add(_titleBox);
        Panel.Children.Add(_artistBox);
        Panel.Children.Add(_yearBox);
        Panel.Children.Add(_formatBox);
        Panel.Children.Add(_labelBox);
        Panel.Children.Add(_catalogBox);
        Panel.Children.Add(_countryBox);
        Panel.Children.Add(_barcodeBox);

        // Per-track editing: title, artist (empty = shares the album artist), side,
        // and track number. Rows align positionally with the seeded tracks and ride
        // back through the same RawReleaseEdit the form round-trips.
        Panel.Children.Add(new TextBlock { Text = Loc.Chrome("edit.tracks.header"), Margin = new Thickness(0, 8, 0, 0) });
        var trackHeader = new Grid { ColumnSpacing = 6 };
        AddTrackColumns(trackHeader);
        HeaderCell(trackHeader, Loc.Chrome("edit.tracks.col_title"), 0);
        HeaderCell(trackHeader, Loc.Chrome("edit.tracks.col_artist"), 1);
        HeaderCell(trackHeader, Loc.Chrome("edit.tracks.col_side"), 2);
        HeaderCell(trackHeader, Loc.Chrome("edit.tracks.col_number"), 3);
        Panel.Children.Add(trackHeader);

        // ErrorText terminates the panel; track rows are inserted just before it so
        // reseeding can rebuild them without disturbing the album fields, header, or
        // error line.
        Panel.Children.Add(ErrorText);

        Seed(seed);
    }

    /// title and artist stretch; side and number are fixed-width.
    private static void AddTrackColumns(Grid g)
    {
        ColumnDefinition Col(double width) =>
            new() { Width = width > 0 ? new GridLength(width) : new GridLength(1, GridUnitType.Star) };
        g.ColumnDefinitions.Add(Col(0));
        g.ColumnDefinitions.Add(Col(0));
        g.ColumnDefinitions.Add(Col(56));
        g.ColumnDefinitions.Add(Col(56));
    }

    private static void HeaderCell(Grid header, string text, int column)
    {
        var cell = new TextBlock { Text = text };
        Grid.SetColumn(cell, column);
        header.Children.Add(cell);
    }

    /// <summary>
    /// Populate the album/pressing fields and rebuild the track table from a freshly
    /// loaded edit, replacing the bound edit. Used for the initial seed and for reset
    /// to source, which discards in-progress edits and re-seeds from the stored
    /// metadata source.
    /// </summary>
    internal void Seed(BridgeRawReleaseEdit e)
    {
        _edit = e;
        _titleBox.Text = e.AlbumTitle;
        _artistBox.Text = e.AlbumArtistText;
        _yearBox.Text = e.Pressing.Year;
        _formatBox.Text = e.Pressing.Format;
        _labelBox.Text = e.Pressing.Label;
        _catalogBox.Text = e.Pressing.CatalogNumber;
        _countryBox.Text = e.Pressing.Country;
        _barcodeBox.Text = e.Pressing.Barcode;

        foreach (var oldRow in _trackRows)
        {
            Panel.Children.Remove(oldRow);
        }
        _trackRows.Clear();
        _trackBoxes.Clear();

        var errorIndex = Panel.Children.IndexOf(ErrorText);
        foreach (var track in e.Tracks)
        {
            var row = new Grid { ColumnSpacing = 6 };
            AddTrackColumns(row);
            var titleCell = new TextBox { Text = track.Title };
            var artistCell = new TextBox { Text = track.ArtistText, PlaceholderText = Loc.Chrome("edit.tracks.artist_placeholder") };
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

    /// <summary>
    /// Enable or disable the six pressing fields (year … barcode). A metadata-only
    /// import claim blanks these core-side, so the form disables them to make the
    /// blanking visible; the album title/artists and track table stay editable.
    /// </summary>
    internal void SetPressingFieldsEnabled(bool enabled)
    {
        _yearBox.IsEnabled = enabled;
        _formatBox.IsEnabled = enabled;
        _labelBox.IsEnabled = enabled;
        _catalogBox.IsEnabled = enabled;
        _countryBox.IsEnabled = enabled;
        _barcodeBox.IsEnabled = enabled;
    }

    /// <summary>
    /// Flush the typed values into the generated edit shape.
    /// Shaping/validation happens in Rust (apply-edit or import-candidate); this
    /// only moves the form's strings onto the model.
    /// </summary>
    internal BridgeRawReleaseEdit ReadBack()
    {
        var tracks = new List<BridgeRawTrackEdit>();
        foreach (var (track, titleCell, artistCell, sideCell, numberCell) in _trackBoxes)
        {
            // Keep the seeded side if the field was cleared/garbled (side is
            // required); a blank track number means "no number".
            var sideValue = track.Side;
            if (int.TryParse(sideCell.Text, out var side))
            {
                sideValue = side;
            }
            tracks.Add(new BridgeRawTrackEdit(
                track.Id,
                titleCell.Text,
                artistCell.Text,
                sideValue,
                int.TryParse(numberCell.Text, out var number) ? number : null));
        }

        _edit = new BridgeRawReleaseEdit(
            _titleBox.Text,
            _artistBox.Text,
            new BridgeRawPressingEdit(
                _yearBox.Text,
                _formatBox.Text,
                _labelBox.Text,
                _catalogBox.Text,
                _countryBox.Text,
                _barcodeBox.Text),
            tracks.ToArray());
        return _edit;
    }
}
