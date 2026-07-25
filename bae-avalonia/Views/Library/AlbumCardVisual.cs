using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Data;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Media.Imaging;

namespace Bae.Desktop;

// One album grid tile: the square cover art (with a multi-selection tint and, when
// this is the expanded album, an accent ring), title, artist, and year. Built from
// an Album so the dynamic parts — cover, selection tint, expansion ring — bind
// OneWay to it and update without a row rebuild as the cover loads or the state
// toggles. Every color reads a theme brush, so the tile renders in either OS
// appearance. The grid attaches the click/selection behavior at its own layer.
internal static class AlbumCardVisual
{
    internal static Control Build(Album album, AlbumGridLayout layout)
    {
        // The selection tint fills the card behind the content, toggled 0/1 by the
        // album's SelectionTintOpacity (the tint brush already carries the low
        // alpha), so a recycled card never re-measures on a selection change.
        var tint = new Border { CornerRadius = new CornerRadius(10) };
        tint[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        tint[!Visual.OpacityProperty] = OneWay(album, nameof(Album.SelectionTintOpacity));

        var image = new Image { Stretch = Stretch.UniformToFill };
        image[!Image.SourceProperty] = OneWay(album, nameof(Album.Cover));

        var art = new Border
        {
            CornerRadius = new CornerRadius(12),
            ClipToBounds = true,
            Child = image,
        };
        art[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");

        // The accent ring on the expanded card's cover, toggled (not shown/hidden)
        // by ExpansionRingOpacity so the recycled card never re-measures.
        var ring = new Border
        {
            CornerRadius = new CornerRadius(12),
            BorderThickness = new Thickness(2),
            IsHitTestVisible = false,
        };
        ring[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeAccentBrush");
        ring[!Visual.OpacityProperty] = OneWay(album, nameof(Album.ExpansionRingOpacity));

        var artHost = new SquareBox { Child = new Panel { Children = { art, ring } } };

        var title = new TextBlock
        {
            Text = album.Title,
            FontSize = 15,
            FontWeight = FontWeight.Bold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 8, 0, 0),
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");

        var artist = new TextBlock
        {
            Text = album.Artist,
            FontSize = 13,
            FontWeight = FontWeight.Medium,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        artist[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        // The year line keeps its height even when absent so tiles stay uniform.
        var year = new TextBlock
        {
            Text = album.YearText,
            FontSize = 12,
            FontWeight = FontWeight.Medium,
            Height = 16,
        };
        year[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var content = new StackPanel { Spacing = 2, Margin = new Thickness(6) };
        content.Children.Add(artHost);
        content.Children.Add(title);
        content.Children.Add(artist);
        content.Children.Add(year);

        var card = new Panel
        {
            // The trailing gutter and bottom row gap read as spaced tiles.
            Margin = new Thickness(0, 0, AlbumGridColumns.Gutter, AlbumGridColumns.RowGap),
            // Transparent (not null) so the whole tile, gaps included, is clickable.
            Background = Brushes.Transparent,
            Children = { tint, content },
        };
        // Width is the shared card metric, so a resize that keeps the column count
        // re-sizes every card without a row rebuild.
        card[!Layoutable.WidthProperty] = OneWay(layout, nameof(AlbumGridLayout.CardWidth));
        return card;
    }

    private static IBinding OneWay(object source, string path) =>
        new Binding(path) { Source = source, Mode = BindingMode.OneWay };
}

// Sizes its child to a square whose side is the available width — the album cover
// stays square as the column width flexes, without a per-tile size-changed hook.
internal sealed class SquareBox : Decorator
{
    protected override Size MeasureOverride(Size availableSize)
    {
        var side = double.IsInfinity(availableSize.Width) ? 0 : availableSize.Width;
        Child?.Measure(new Size(side, side));
        return new Size(side, side);
    }

    protected override Size ArrangeOverride(Size finalSize)
    {
        Child?.Arrange(new Rect(finalSize));
        return finalSize;
    }
}
