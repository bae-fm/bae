using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The mapping pane's shared leaves — section titles, column headers, cells, the
/// mono file name with its dimmed directory prefix, and the small row button.
/// Split out so the pane's two sections read as their own structure rather than
/// as two copies of the same Border-and-TextBlock spelling.
/// </summary>
internal readonly record struct ManualSearchSourceSelection(
    bool MusicBrainz,
    bool Discogs)
{
    internal BridgeSearchSources? QuerySources => (MusicBrainz, Discogs) switch
    {
        (true, true) => new BridgeSearchSources.Both(),
        (true, false) => new BridgeSearchSources.One(BridgeMetadataSource.MusicBrainz),
        (false, true) => new BridgeSearchSources.One(BridgeMetadataSource.Discogs),
        (false, false) => null,
    };
}

internal static class ImportPaneUi
{
    /// <summary>A section's heading, with an optional plain note beside it (the
    /// reconciliation tally).</summary>
    internal static Control ZoneTitle(string text, string? note = null)
    {
        var title = new TextBlock
        {
            Text = text,
            FontSize = 12.5,
            FontWeight = FontWeight.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");

        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 10 };
        row.Children.Add(title);
        if (!string.IsNullOrEmpty(note))
        {
            var noteText = new TextBlock { Text = note, FontSize = 12, VerticalAlignment = VerticalAlignment.Center };
            noteText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            row.Children.Add(noteText);
        }
        return new Border { Margin = new Thickness(0, 0, 0, 6), Child = row };
    }

    internal static Control ColumnHeader(string text, int column)
    {
        var cell = new TextBlock
        {
            Text = text.ToUpper(System.Globalization.CultureInfo.CurrentUICulture),
            FontSize = 10,
            FontWeight = FontWeight.SemiBold,
            LetterSpacing = 1.1,
        };
        cell[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        Grid.SetColumn(cell, column);
        return cell;
    }

    internal static TextBlock Cell(string? text, bool secondary = false)
    {
        var cell = new TextBlock
        {
            Text = text,
            FontSize = 12.5,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        cell[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension(secondary ? "BaeTextSecondaryBrush" : "BaeTextPrimaryBrush");
        return cell;
    }

    /// <summary>A file, the way both tables show one: its directory prefix
    /// dimmed ahead of its name in a mono face, with the size after it.</summary>
    internal static Control FileName(string? dirPrefix, string fileName, long sizeBytes)
    {
        var line = new StackPanel { Orientation = Orientation.Horizontal, VerticalAlignment = VerticalAlignment.Center };
        if (!string.IsNullOrEmpty(dirPrefix))
        {
            var prefix = new TextBlock
            {
                Text = dirPrefix,
                FontFamily = new FontFamily("monospace"),
                FontSize = 12,
                VerticalAlignment = VerticalAlignment.Center,
            };
            prefix[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            line.Children.Add(prefix);
        }
        var name = new TextBlock
        {
            Text = fileName,
            FontFamily = new FontFamily("monospace"),
            FontSize = 12,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        line.Children.Add(name);

        var size = new TextBlock
        {
            Text = Loc.Bytes(sizeBytes),
            FontSize = 11.5,
            Margin = new Thickness(8, 0, 0, 0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        size[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        line.Children.Add(size);
        return line;
    }

    /// <summary>The small outlined button both tables hang their row actions
    /// off.</summary>
    internal static Button RowButton(string label)
    {
        var button = new Button
        {
            Content = label,
            FontSize = 11.5,
            Padding = new Thickness(9, 3),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(1),
            Background = Brushes.Transparent,
            VerticalAlignment = VerticalAlignment.Center,
        };
        button[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return button;
    }

    internal static StackPanel MetadataSourceToggles(
        ManualSearchSourceSelection selection,
        bool discogsEnabled,
        out CheckBox musicBrainz,
        out CheckBox discogs)
    {
        musicBrainz = new CheckBox
        {
            Content = "MusicBrainz",
            IsChecked = selection.MusicBrainz,
        };
        discogs = new CheckBox
        {
            Content = "Discogs",
            IsChecked = discogsEnabled && selection.Discogs,
            IsEnabled = discogsEnabled,
        };
        return new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            Children = { musicBrainz, discogs },
        };
    }
}
