using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The empty-library shell (desktop story 3): the chrome around an empty library.
// Across the top a Library/Import switcher, a search field, and a settings gear;
// below, a large bold "Albums" heading that is itself a mode dropdown with sort
// controls opposite; the content area's text-only empty state; and an idle
// now-playing bar docked along the bottom. Every color reads a theme brush, so
// the shell renders in either OS appearance. The library grid, the real switcher
// wiring, and the live now-playing transport arrive with the parity port; this is
// the shell the story checks.
internal sealed class MainShellView : UserControl
{
    private readonly AppService _app;

    public MainShellView(AppService app)
    {
        _app = app;

        var root = new Grid { RowDefinitions = new RowDefinitions("Auto,*,Auto") };
        SetBg(root, "BaeBackgroundBrush");

        var toolbar = BuildToolbar();
        Grid.SetRow(toolbar, 0);
        root.Children.Add(toolbar);

        var content = BuildContent();
        Grid.SetRow(content, 1);
        root.Children.Add(content);

        var bar = BuildNowPlayingBar();
        Grid.SetRow(bar, 2);
        root.Children.Add(bar);

        Content = root;
    }

    // ── Toolbar ──────────────────────────────────────────────────────────────
    private static Control BuildToolbar()
    {
        var strip = new Border
        {
            Height = 56,
            BorderThickness = new Thickness(0, 0, 0, 1),
        };
        SetBg(strip, "BaeSurfaceBrush");
        SetBorder(strip, "BaeHairlineBrush");

        var grid = new Grid { Margin = new Thickness(16, 0), VerticalAlignment = VerticalAlignment.Center };

        // The Library/Import switcher: a segmented pill centered in the bar, the
        // active segment filled with the accent.
        var pill = new Border
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            CornerRadius = new CornerRadius(11),
            Padding = new Thickness(4),
            BorderThickness = new Thickness(1),
        };
        SetBg(pill, "BaeFieldBrush");
        SetBorder(pill, "BaeHairlineBrush");
        var segments = new StackPanel { Orientation = Orientation.Horizontal };
        segments.Children.Add(Segment(Loc.Chrome("section.library"), active: true));
        segments.Children.Add(Segment(Loc.Chrome("section.import"), active: false));
        pill.Child = segments;
        grid.Children.Add(pill);

        // Right cluster: the search field and the settings gear.
        var right = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Center,
        };
        right.Children.Add(BuildSearchField());
        right.Children.Add(Icons.IconButton(Icons.Gear, 18, "BaeTextSecondaryBrush", 34));
        grid.Children.Add(right);

        strip.Child = grid;
        return strip;
    }

    private static Control Segment(string text, bool active)
    {
        var segment = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(20, 7),
        };
        var label = new TextBlock
        {
            Text = text,
            FontSize = 14.5,
            FontWeight = FontWeight.Bold,
        };
        if (active)
        {
            SetBg(segment, "BaeAccentBrush");
            label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeOnAccentBrush");
        }
        else
        {
            label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        }
        segment.Child = label;
        return segment;
    }

    private static Control BuildSearchField()
    {
        var field = new Border
        {
            Width = 300,
            Height = 38,
            CornerRadius = new CornerRadius(10),
            Padding = new Thickness(12, 0),
        };
        SetBg(field, "BaeFieldBrush");
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Center,
        };
        row.Children.Add(Icons.Glyph(Icons.Search, 16, "BaeTextSecondaryBrush"));
        var placeholder = new TextBlock
        {
            Text = Loc.Chrome("SearchBox.PlaceholderText"),
            VerticalAlignment = VerticalAlignment.Center,
        };
        placeholder[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        row.Children.Add(placeholder);
        field.Child = row;
        return field;
    }

    // ── Content column: heading + sort + empty state ─────────────────────────
    private Control BuildContent()
    {
        var column = new Grid
        {
            MaxWidth = 1240,
            HorizontalAlignment = HorizontalAlignment.Stretch,
            RowDefinitions = new RowDefinitions("Auto,*"),
        };

        var header = new Grid
        {
            Margin = new Thickness(22, 56, 22, 32),
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
        };

        // The mode heading is the mode switcher: a large heavy label with a chevron
        // whose dropdown picks albums / artists / composers.
        var heading = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
        };
        var headingText = new TextBlock
        {
            Text = Loc.Chrome("library.mode.albums"),
            FontSize = 56,
            FontWeight = FontWeight.ExtraBold,
            VerticalAlignment = VerticalAlignment.Bottom,
        };
        headingText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        heading.Children.Add(headingText);
        var chevron = Icons.Glyph(Icons.ChevronDown, 18, "BaeTextSecondaryBrush");
        chevron.VerticalAlignment = VerticalAlignment.Bottom;
        chevron.Margin = new Thickness(0, 0, 0, 12);
        heading.Children.Add(chevron);
        var headingButton = new Button
        {
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Bottom,
            Content = heading,
        };
        Grid.SetColumn(headingButton, 0);
        header.Children.Add(headingButton);

        var sort = BuildSortControls();
        Grid.SetColumn(sort, 1);
        header.Children.Add(sort);

        Grid.SetRow(header, 0);
        column.Children.Add(header);

        var empty = BuildEmptyState();
        Grid.SetRow(empty, 1);
        column.Children.Add(empty);

        return column;
    }

    private static Control BuildSortControls()
    {
        var controls = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Bottom,
        };

        // The active sort criterion as a chip, plus the direction toggle — the
        // sort controls sitting opposite the mode heading.
        var chip = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(12, 6),
            BorderThickness = new Thickness(1),
        };
        SetBg(chip, "BaeElevatedBrush");
        SetBorder(chip, "BaeHairlineBrush");
        var chipText = new TextBlock { Text = Loc.Chrome("sort.title"), FontSize = 13 };
        chipText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        chip.Child = chipText;
        controls.Children.Add(chip);

        var direction = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(7),
            BorderThickness = new Thickness(1),
        };
        SetBg(direction, "BaeElevatedBrush");
        SetBorder(direction, "BaeHairlineBrush");
        direction.Child = Icons.Glyph(Icons.ArrowUp, 16, "BaeTextSecondaryBrush");
        controls.Children.Add(direction);

        return controls;
    }

    private Control BuildEmptyState()
    {
        // The zero-count empty-state branch, read off the (stubbed or live)
        // library exactly as the grid load will: the albums heading over an empty
        // library shows "No albums". The library grid replaces this content once a
        // count is non-zero, which lands with the incremental-loading redesign.
        var (_, albumCount) = _app.Library.AlbumCount();

        var stack = new StackPanel
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 6,
            IsVisible = albumCount == 0,
        };
        var title = new TextBlock
        {
            Text = Loc.Chrome("library.empty"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontSize = 24,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var guidance = new TextBlock
        {
            Text = Loc.Chrome("library.empty_guidance"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontSize = 14,
        };
        guidance[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        stack.Children.Add(title);
        stack.Children.Add(guidance);
        return stack;
    }

    // ── Idle now-playing bar ─────────────────────────────────────────────────
    private static Control BuildNowPlayingBar()
    {
        var bar = new Border { BorderThickness = new Thickness(0, 1, 0, 0) };
        SetBg(bar, "BaeSurfaceBrush");
        SetBorder(bar, "BaeHairlineBrush");

        var grid = new Grid
        {
            Margin = new Thickness(20, 12),
            VerticalAlignment = VerticalAlignment.Center,
            ColumnDefinitions = new ColumnDefinitions("*,Auto,*"),
        };

        // Left: cover placeholder and the (empty, idle) title/artist.
        var left = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            VerticalAlignment = VerticalAlignment.Center,
        };
        var cover = new Border { Width = 54, Height = 54, CornerRadius = new CornerRadius(10) };
        SetBg(cover, "BaeElevatedBrush");
        left.Children.Add(cover);
        Grid.SetColumn(left, 0);
        grid.Children.Add(left);

        // Center: transport row above the scrubber row, fixed width.
        var center = new StackPanel { Width = 460, VerticalAlignment = VerticalAlignment.Center, Spacing = 6 };
        var transport = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 22,
            HorizontalAlignment = HorizontalAlignment.Center,
        };
        transport.Children.Add(Icons.IconButton(Icons.Shuffle, 16, "BaeTextSecondaryBrush", 30));
        transport.Children.Add(Icons.IconButton(Icons.SkipPrevious, 20, "BaeTextPrimaryBrush", 34));
        transport.Children.Add(BuildPlayButton());
        transport.Children.Add(Icons.IconButton(Icons.SkipNext, 20, "BaeTextPrimaryBrush", 34));
        transport.Children.Add(Icons.IconButton(Icons.Repeat, 16, "BaeTextSecondaryBrush", 30));
        center.Children.Add(transport);
        center.Children.Add(BuildScrubber());
        Grid.SetColumn(center, 1);
        grid.Children.Add(center);

        // Right: cast, queue, mute stand-in, volume — trailing-aligned.
        var right = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Center,
        };
        right.Children.Add(Icons.IconButton(Icons.Cast, 16, "BaeTextSecondaryBrush", 30));
        right.Children.Add(Icons.IconButton(Icons.Queue, 17, "BaeTextSecondaryBrush", 32));
        right.Children.Add(Icons.IconButton(Icons.VolumeUp, 16, "BaeTextSecondaryBrush", 30));
        var volume = new Slider { Minimum = 0, Maximum = 1, Value = 0.7, Width = 96, VerticalAlignment = VerticalAlignment.Center };
        right.Children.Add(volume);
        Grid.SetColumn(right, 2);
        grid.Children.Add(right);

        bar.Child = grid;
        return bar;
    }

    private static Control BuildPlayButton()
    {
        var circle = new Border
        {
            Width = 48,
            Height = 48,
            CornerRadius = new CornerRadius(24),
        };
        SetBg(circle, "BaeAccentBrush");
        var glyph = Icons.Glyph(Icons.Play, 20, "BaeOnAccentBrush");
        glyph.HorizontalAlignment = HorizontalAlignment.Center;
        glyph.VerticalAlignment = VerticalAlignment.Center;
        circle.Child = glyph;
        return circle;
    }

    private static Control BuildScrubber()
    {
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto"), ColumnSpacing = 11 };
        var elapsed = TimeLabel("0:00");
        elapsed.TextAlignment = TextAlignment.Right;
        Grid.SetColumn(elapsed, 0);
        grid.Children.Add(elapsed);

        var progress = new Slider { Minimum = 0, Maximum = 1, Value = 0, VerticalAlignment = VerticalAlignment.Center };
        Grid.SetColumn(progress, 1);
        grid.Children.Add(progress);

        var duration = TimeLabel("0:00");
        Grid.SetColumn(duration, 2);
        grid.Children.Add(duration);
        return grid;
    }

    private static TextBlock TimeLabel(string text)
    {
        var label = new TextBlock
        {
            Text = text,
            MinWidth = 34,
            VerticalAlignment = VerticalAlignment.Center,
            FontSize = 11.5,
            FontWeight = FontWeight.SemiBold,
        };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return label;
    }

    private static void SetBg(Border border, string key) =>
        border[!Border.BackgroundProperty] = new DynamicResourceExtension(key);

    private static void SetBg(Panel panel, string key) =>
        panel[!Panel.BackgroundProperty] = new DynamicResourceExtension(key);

    private static void SetBorder(Border border, string key) =>
        border[!Border.BorderBrushProperty] = new DynamicResourceExtension(key);
}
