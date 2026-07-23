using System.Numerics;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// The album grid card's visual, built from display values so the live grid and
// the static previews build the identical tile. MainWindow calls Build with an
// album's current values, then live-binds the dynamic parts (width, tint,
// expansion ring, cover) and wires the click / right-tap / drag at its layer; the
// capture scene and the component gallery call it with fixtures and take the
// static card as-is. No Album, handle, or grid-layout dependency — values in,
// visual out — the WinUI analogue of the macOS card view over a stub-backed
// preview.
internal static class AlbumCardVisual
{
    // A built card and the elements a live host re-binds: the selection tint, the
    // expansion ring, and the cover image. The static callers ignore these and
    // keep the values passed to Build.
    internal readonly record struct Parts(Grid Card, Border Tint, Border Ring, Image Cover);

    // Build one album tile: the cover art (with a selection tint and, when this is
    // the expanded album, an accent ring), title, artist, and year. Width, the two
    // opacities, and the cover are the dynamic values the live grid overrides with
    // OneWay bindings; the accent colors the tint and ring.
    internal static Parts Build(
        string title,
        string artist,
        string year,
        ImageSource? cover,
        double cardWidth,
        double selectionTintOpacity,
        double expansionRingOpacity,
        global::Windows.UI.Color accent)
    {
        var tint = new Border
        {
            CornerRadius = new CornerRadius(10),
            Background = new SolidColorBrush(accent) { Opacity = 0.22 },
            Opacity = selectionTintOpacity,
        };

        var art = new Border
        {
            CornerRadius = new CornerRadius(12),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            Background = (Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Translation = new Vector3(0, 0, 16),
            Shadow = new ThemeShadow(),
        };
        art.SizeChanged += KeepArtSquare;
        var image = new Image { Stretch = Stretch.UniformToFill, Source = cover };
        art.Child = image;

        var ring = new Border
        {
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(2),
            BorderBrush = new SolidColorBrush(accent),
            IsHitTestVisible = false,
            Opacity = expansionRingOpacity,
        };

        var artHost = new Grid { HorizontalAlignment = HorizontalAlignment.Stretch };
        artHost.Children.Add(art);
        artHost.Children.Add(ring);

        var titleText = new TextBlock
        {
            Text = title,
            FontSize = 15,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 8, 0, 0),
        };
        var artistText = new TextBlock
        {
            Text = artist,
            FontSize = 13,
            FontWeight = Microsoft.UI.Text.FontWeights.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Foreground = (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        };
        // The year line keeps its height even when absent so tiles stay uniform.
        var yearText = new TextBlock
        {
            Text = year,
            FontSize = 12,
            FontWeight = Microsoft.UI.Text.FontWeights.Medium,
            Height = 16,
            Foreground = (Brush)Application.Current.Resources["TextFillColorTertiaryBrush"],
        };

        var content = new StackPanel { Spacing = 2, Padding = new Thickness(6) };
        content.Children.Add(artHost);
        content.Children.Add(titleText);
        content.Children.Add(artistText);
        content.Children.Add(yearText);

        var card = new Grid
        {
            // The trailing gutter and bottom row gap read as spaced tiles, as
            // ItemsWrapGrid's equal cells did.
            Margin = new Thickness(0, 0, AlbumGridColumns.Gutter, AlbumGridColumns.RowGap),
            Width = cardWidth,
        };
        card.Children.Add(tint);
        card.Children.Add(content);

        return new Parts(card, tint, ring, image);
    }

    // Keep each tile's art square: match its height to its (stretched) width.
    private static void KeepArtSquare(object sender, SizeChangedEventArgs e)
    {
        if (sender is FrameworkElement art
            && (double.IsNaN(art.Height) || System.Math.Abs(art.Height - e.NewSize.Width) > 0.5))
        {
            art.Height = e.NewSize.Width;
        }
    }
}
