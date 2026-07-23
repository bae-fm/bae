#if DEBUG
using System;
using System.Collections.Generic;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// A debug-only gallery window: a sidebar of reusable views/components rendered
// against static fixtures (PreviewData), the WinUI analogue of the macOS SwiftUI
// previews. WinUI has no design-time preview, so the loop is Hot Reload plus
// reopening this window. Only leaves that render pure data are listed here —
// anything needing a live library handle (cover images, the settings sections,
// the browse/queue/storage panes) is left out; see the reorg report for the
// skipped set. Compiled only in DEBUG builds.
internal sealed class ComponentGalleryWindow : Window
{
    // One gallery entry: a developer-facing label (the component's own name, not
    // localized product chrome) and a builder that renders it against fixtures.
    private readonly record struct Entry(string Label, Func<FrameworkElement> Build);

    private static readonly IReadOnlyList<Entry> Entries = new[]
    {
        new Entry("SignalBadgeRow", BuildSignalBadges),
        new Entry("DialogPrimitives · storage labels", BuildStorageLabels),
        new Entry("SaveFilenameTokenDisplay", BuildFilenameTokens),
        new Entry("DialogPrimitives · code display", BuildCodeDisplay),
        new Entry("DialogPrimitives · cover tiles", BuildCoverTiles),
    };

    public ComponentGalleryWindow()
    {
        Title = Loc.Chrome("component_gallery.title");

        var sidebar = new ListView { SelectionMode = ListViewSelectionMode.Single, Width = 260 };
        foreach (var entry in Entries)
        {
            sidebar.Items.Add(entry.Label);
        }

        var host = new ContentControl
        {
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            VerticalContentAlignment = VerticalAlignment.Stretch,
        };
        sidebar.SelectionChanged += (_, _) =>
        {
            if (sidebar.SelectedIndex >= 0)
            {
                host.Content = new ScrollViewer { Content = Entries[sidebar.SelectedIndex].Build() };
            }
        };
        sidebar.SelectedIndex = 0;

        var grid = new Grid();
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var divider = new Border
        {
            BorderThickness = new Thickness(1, 0, 0, 0),
            BorderBrush = (Brush)Application.Current.Resources["DividerStrokeColorDefaultBrush"],
            Child = host,
        };
        Grid.SetColumn(sidebar, 0);
        Grid.SetColumn(divider, 1);
        grid.Children.Add(sidebar);
        grid.Children.Add(divider);
        Content = grid;
    }

    private static TextBlock Header(string text) => new()
    {
        Text = text,
        FontSize = 18,
        FontWeight = FontWeights.SemiBold,
        Margin = new Thickness(0, 0, 0, 8),
    };

    private static StackPanel Section() => new() { Spacing = 8, Padding = new Thickness(24) };

    private static FrameworkElement BuildSignalBadges()
    {
        var panel = Section();
        panel.Children.Add(Header("SignalBadgeRow.Build"));
        panel.Children.Add(SignalBadgeRow.Build(PreviewData.SignalBadges, (_, _) => { }, () => { }));
        return panel;
    }

    private static FrameworkElement BuildStorageLabels()
    {
        var panel = Section();
        panel.Children.Add(Header("DialogPrimitives.StorageActionLabel / RestingStorageLabel"));
        foreach (var action in new[]
        {
            BridgeReleaseStorageAction.MakeRemote,
            BridgeReleaseStorageAction.MakeLocal,
            BridgeReleaseStorageAction.Pin,
            BridgeReleaseStorageAction.Unpin,
        })
        {
            panel.Children.Add(new TextBlock { Text = DialogPrimitives.StorageActionLabel(action) });
        }
        panel.Children.Add(new TextBlock { Text = DialogPrimitives.RestingStorageLabel(isManaged: true, pinned: false) });
        panel.Children.Add(new TextBlock { Text = DialogPrimitives.RestingStorageLabel(isManaged: true, pinned: true) });
        panel.Children.Add(new TextBlock { Text = DialogPrimitives.RestingStorageLabel(isManaged: false, pinned: false) });
        return panel;
    }

    private static FrameworkElement BuildFilenameTokens()
    {
        var panel = Section();
        panel.Children.Add(Header("SaveFilenameTokenDisplay"));
        foreach (var token in SaveFilenameTokenDisplay.All)
        {
            panel.Children.Add(new TextBlock
            {
                Text = $"{SaveFilenameTokenDisplay.Label(token)} → {SaveFilenameTokenDisplay.Sample(token)}",
            });
        }
        panel.Children.Add(new TextBlock
        {
            Text = SaveFilenameTokenDisplay.PreviewFilename(SaveFilenameTokenDisplay.All, "flac"),
            FontFamily = new FontFamily("Consolas"),
            Margin = new Thickness(0, 8, 0, 0),
        });
        return panel;
    }

    private static FrameworkElement BuildCodeDisplay()
    {
        var panel = Section();
        panel.Children.Add(Header("DialogPrimitives.BuildCodeDisplay"));
        panel.Children.Add(DialogPrimitives.BuildCodeDisplay(PreviewData.SampleCode));
        return panel;
    }

    private static FrameworkElement BuildCoverTiles()
    {
        var panel = Section();
        panel.Children.Add(Header("DialogPrimitives.CoverTile (placeholder art)"));
        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 12 };
        row.Children.Add(DialogPrimitives.CoverTile(null, "Cover A"));
        row.Children.Add(DialogPrimitives.CoverTile(null, "Cover B"));
        panel.Children.Add(row);
        return panel;
    }
}
#endif
