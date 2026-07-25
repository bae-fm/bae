using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The album expansion's pure-data visual pieces: the header block and one track
// row. Display values and action callbacks in, visuals out — no session or bridge
// access, so they render standalone. Every color reads a theme brush.
internal static class AlbumExpansionRows
{
    // The header block: the album title over its artist.
    public static Control BuildHeaderBlock(string title, string artist)
    {
        var stack = new StackPanel { Spacing = 2 };
        var titleText = new TextBlock
        {
            Text = title,
            FontSize = 24,
            FontWeight = FontWeight.Bold,
            MaxLines = 2,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        titleText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var artistText = new TextBlock
        {
            Text = artist,
            FontSize = 15,
            FontWeight = FontWeight.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        artistText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        stack.Children.Add(titleText);
        stack.Children.Add(artistText);
        return stack;
    }

    // One track row: position, title (with the row artist under it when core names
    // one — a compilation), and duration. A left click plays the release from this
    // track; a right-click opens the per-track menu (play / play next / add to
    // queue). The callbacks are the panel's.
    public static Control BuildTrackRow(
        string position,
        string title,
        string? artist,
        string duration,
        Action onPlay,
        Action onPlayNext,
        Action onAddToQueue)
    {
        var positionText = new TextBlock
        {
            Text = position,
            VerticalAlignment = VerticalAlignment.Center,
            MinWidth = 24,
        };
        positionText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var textStack = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        var titleText = new TextBlock
        {
            Text = title,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        titleText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        textStack.Children.Add(titleText);
        if (!string.IsNullOrEmpty(artist))
        {
            var artistText = new TextBlock
            {
                Text = artist,
                FontSize = 12,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
            };
            artistText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            textStack.Children.Add(artistText);
        }

        var durationText = new TextBlock
        {
            Text = duration,
            VerticalAlignment = VerticalAlignment.Center,
        };
        durationText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        var grid = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto"),
            ColumnSpacing = 12,
            Margin = new Thickness(4, 6),
            // Transparent (not null) so the whole row, gaps included, is clickable.
            Background = Brushes.Transparent,
        };
        Grid.SetColumn(positionText, 0);
        Grid.SetColumn(textStack, 1);
        Grid.SetColumn(durationText, 2);
        grid.Children.Add(positionText);
        grid.Children.Add(textStack);
        grid.Children.Add(durationText);

        grid.PointerPressed += (_, e) =>
        {
            if (e.GetCurrentPoint(grid).Properties.IsLeftButtonPressed)
            {
                onPlay();
            }
        };
        grid.ContextMenu = new ContextMenu
        {
            ItemsSource = new[]
            {
                MenuItem(Loc.Chrome("menu.play"), onPlay),
                MenuItem(Loc.Chrome("menu.play_next"), onPlayNext),
                MenuItem(Loc.Chrome("menu.add_to_queue"), onAddToQueue),
            },
        };
        return grid;
    }

    private static MenuItem MenuItem(string header, Action onClick)
    {
        var item = new MenuItem { Header = header };
        item.Click += (_, _) => onClick();
        return item;
    }
}
