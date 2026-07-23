using System;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Bae.Windows;

// The album expansion's pure-data visual pieces, factored out of
// AlbumExpansionPanel.BuildAsync so the panel renders them from live detail and
// the debug component gallery renders them from PreviewData fixtures. Display
// values and action callbacks in, visuals out — no bridge or handle access, so
// they render standalone.
internal static class AlbumExpansionRows
{
    // The header block: the album title over its artist. The action buttons and
    // release picker are not here — those carry live per-release callbacks.
    public static FrameworkElement BuildHeaderBlock(string title, string artist)
    {
        var stack = new StackPanel { Spacing = 2 };
        stack.Children.Add(new TextBlock
        {
            Text = title,
            FontSize = 24,
            FontWeight = Microsoft.UI.Text.FontWeights.Bold,
            MaxLines = 2,
            TextTrimming = TextTrimming.CharacterEllipsis,
        });
        stack.Children.Add(new TextBlock
        {
            Text = artist,
            FontSize = 15,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = Secondary,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        });
        return stack;
    }

    // One track row: position, title (with the row artist under it when core names
    // one — a compilation), and duration. A left click plays the release from this
    // track; a right-tap opens the per-track menu (play / play next / add to queue
    // / save as). The callbacks are the panel's; the gallery passes no-ops.
    public static FrameworkElement BuildTrackRow(
        string position,
        string title,
        string? artist,
        string duration,
        Action onPlay,
        Action onPlayNext,
        Action onAddToQueue,
        Action onExportTrack)
    {
        var positionText = new TextBlock
        {
            Text = position,
            Foreground = Secondary,
            VerticalAlignment = VerticalAlignment.Center,
            MinWidth = 24,
        };
        var textStack = new StackPanel { Spacing = 1, VerticalAlignment = VerticalAlignment.Center };
        textStack.Children.Add(new TextBlock
        {
            Text = title,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        });
        if (!string.IsNullOrEmpty(artist))
        {
            textStack.Children.Add(new TextBlock
            {
                Text = artist,
                FontSize = 12,
                Foreground = Secondary,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
            });
        }
        var durationText = new TextBlock
        {
            Text = duration,
            Foreground = Secondary,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var grid = new Grid
        {
            ColumnSpacing = 12,
            Padding = new Thickness(4, 6, 4, 6),
            // Transparent (not null) so the whole row, gaps included, is hit-testable.
            Background = new SolidColorBrush(Colors.Transparent),
        };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(positionText, 0);
        Grid.SetColumn(textStack, 1);
        Grid.SetColumn(durationText, 2);
        grid.Children.Add(positionText);
        grid.Children.Add(textStack);
        grid.Children.Add(durationText);
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(grid, title);

        grid.Tapped += (_, _) => onPlay();
        grid.RightTapped += (sender, args) =>
        {
            var menu = new MenuFlyout();
            var play = new MenuFlyoutItem { Text = Loc.Chrome("menu.play") };
            play.Click += (_, _) => onPlay();
            var playNext = new MenuFlyoutItem { Text = Loc.Chrome("menu.play_next") };
            playNext.Click += (_, _) => onPlayNext();
            var addToQueue = new MenuFlyoutItem { Text = Loc.Chrome("menu.add_to_queue") };
            addToQueue.Click += (_, _) => onAddToQueue();
            var save = new MenuFlyoutItem { Text = Loc.Chrome("menu.save_as") };
            save.Click += (_, _) => onExportTrack();
            menu.Items.Add(play);
            menu.Items.Add(playNext);
            menu.Items.Add(addToQueue);
            menu.Items.Add(save);
            var element = (FrameworkElement)sender;
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
            args.Handled = true;
        };
        return grid;
    }

    private static Brush Secondary => (Brush)Application.Current.Resources["TextFillColorSecondaryBrush"];
}
