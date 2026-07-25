using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The library shell (desktop story 3 in its empty state): the chrome around an
// open library. Across the top a Library/Import switcher, a search field, and a
// settings gear; below, either the library browser (a large bold mode heading
// that is itself a mode dropdown with sort controls opposite, over the grid) or
// the import section, swapped by the switcher; a docked queue sidebar; and a
// now-playing bar along the bottom. Every color reads a theme brush, so the shell
// renders in either OS appearance. The live now-playing transport arrives with a
// later step; this is the shell the story checks.
internal sealed class MainShellView : UserControl
{
    private readonly AppService _app;
    private readonly StorageDialog _storageDialog;
    private readonly SettingsWindow _settingsWindow;
    private readonly LibrariesDialog _librariesDialog;
    private readonly System.Func<System.Threading.Tasks.Task> _closeLibrary;
    private readonly LibraryBrowserView _browser;
    private readonly ImportSectionView _importSection;
    private readonly QueuePane _queuePane;
    private Button? _queueButton;

    // The two switcher segments, restyled as the active section changes.
    private Border _librarySegment = null!;
    private TextBlock _libraryLabel = null!;
    private Border _importSegment = null!;
    private TextBlock _importLabel = null!;

    public MainShellView(
        AppService app,
        ReleaseActionDialogs dialogs,
        ImportDialogs importDialogs,
        StorageDialog storageDialog,
        SettingsWindow settingsWindow,
        LibrariesDialog librariesDialog,
        System.Func<System.Threading.Tasks.Task> closeLibrary)
    {
        _app = app;
        _storageDialog = storageDialog;
        _settingsWindow = settingsWindow;
        _librariesDialog = librariesDialog;
        _closeLibrary = closeLibrary;

        var root = new Grid { RowDefinitions = new RowDefinitions("Auto,*,Auto") };
        SetBg(root, "BaeBackgroundBrush");

        var toolbar = BuildToolbar();
        Grid.SetRow(toolbar, 0);
        root.Children.Add(toolbar);

        // The content row docks the active section beside the queue sidebar: the
        // section takes the remaining width and the queue host reflows when open.
        // The browser and import section both live here; the switcher toggles which
        // is visible.
        var contentRow = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        _browser = new LibraryBrowserView(_app, dialogs);
        _importSection = new ImportSectionView(_app, importDialogs) { IsVisible = false };
        var section = new Panel();
        section.Children.Add(_browser);
        section.Children.Add(_importSection);
        Grid.SetColumn(section, 0);
        var queueHost = new Border();
        Grid.SetColumn(queueHost, 1);
        contentRow.Children.Add(section);
        contentRow.Children.Add(queueHost);
        Grid.SetRow(contentRow, 1);
        root.Children.Add(contentRow);

        var bar = BuildNowPlayingBar();
        Grid.SetRow(bar, 2);
        root.Children.Add(bar);

        Content = root;

        _queuePane = new QueuePane(_app, queueHost);
        if (_queueButton is not null)
        {
            _queuePane.AttachToggle(_queueButton);
        }

        // The cast picker requeries its device list on a castDevices invalidation
        // (discovery only runs while the picker is open, but the projection stays
        // registered for the window's life).
        _app.ProjectionRegistry.Register(
            typeof(uniffi.bae_bridge.BridgeInvalidation.CastDevices), _app.CastStore.RefreshDevices);
    }

    // Switch to the import section and land it on a fresh New tab — the switcher
    // click, and the folder-drop / activation flows that route here.
    public void ShowImport()
    {
        _browser.IsVisible = false;
        _importSection.IsVisible = true;
        SetActiveSection(import: true);
        _importSection.OnEntered();
    }

    public void ShowLibrary()
    {
        _importSection.IsVisible = false;
        _browser.IsVisible = true;
        SetActiveSection(import: false);
    }

    // Reveal an album in the library grid (the import confirm's "view in library"
    // jump): switch to the library section, then page the album in and expand it.
    public System.Threading.Tasks.Task OpenAlbum(string albumId)
    {
        ShowLibrary();
        return _browser.RevealAlbum(albumId);
    }

    // ── Toolbar ──────────────────────────────────────────────────────────────
    private Control BuildToolbar()
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
        (_librarySegment, _libraryLabel) = BuildSegment(Loc.Chrome("section.library"));
        (_importSegment, _importLabel) = BuildSegment(Loc.Chrome("section.import"));
        _librarySegment.PointerPressed += (_, _) => ShowLibrary();
        _importSegment.PointerPressed += (_, _) => ShowImport();
        segments.Children.Add(_librarySegment);
        segments.Children.Add(_importSegment);
        pill.Child = segments;
        grid.Children.Add(pill);
        SetActiveSection(import: false);

        // Right cluster: the search field and the settings gear.
        var right = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Center,
        };
        right.Children.Add(BuildSearchField());
        // The gear opens Settings on click; its right-click flyout carries the
        // commands macOS keeps in its menu bar — Storage today, the rest as their
        // flows migrate.
        var gear = Icons.IconButton(Icons.Gear, 18, "BaeTextSecondaryBrush", 34);
        gear.Click += (_, _) => _settingsWindow.Show();
        gear.ContextFlyout = BuildGearMenu();
        right.Children.Add(gear);
        grid.Children.Add(right);

        strip.Child = grid;
        return strip;
    }

    private MenuFlyout BuildGearMenu()
    {
        var menu = new MenuFlyout();

        var libraries = new MenuItem { Header = Loc.Chrome("toolbar.libraries") };
        libraries.Click += (_, _) => _ = _librariesDialog.Show();
        menu.Items.Add(libraries);

        var storage = new MenuItem { Header = Loc.Chrome("toolbar.storage") };
        storage.Click += (_, _) => _ = _storageDialog.Show();
        menu.Items.Add(storage);

        menu.Items.Add(new Separator());

        var close = new MenuItem { Header = Loc.Chrome("toolbar.close_library") };
        close.Click += (_, _) => _ = _closeLibrary();
        menu.Items.Add(close);

        return menu;
    }

    private static (Border Segment, TextBlock Label) BuildSegment(string text)
    {
        var segment = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(20, 7),
            Background = Brushes.Transparent,
        };
        var label = new TextBlock
        {
            Text = text,
            FontSize = 14.5,
            FontWeight = FontWeight.Bold,
        };
        segment.Child = label;
        return (segment, label);
    }

    // Fill the active segment with the accent and de-emphasize the other, matching
    // the visible section.
    private void SetActiveSection(bool import)
    {
        StyleSegment(_librarySegment, _libraryLabel, active: !import);
        StyleSegment(_importSegment, _importLabel, active: import);
    }

    private static void StyleSegment(Border segment, TextBlock label, bool active)
    {
        if (active)
        {
            SetBg(segment, "BaeAccentBrush");
            label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeOnAccentBrush");
        }
        else
        {
            segment.Background = Brushes.Transparent;
            label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        }
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

    // ── Idle now-playing bar ─────────────────────────────────────────────────
    private Control BuildNowPlayingBar()
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
        right.Children.Add(new CastButton(_app.CastStore));
        _queueButton = Icons.IconButton(Icons.Queue, 17, "BaeTextSecondaryBrush", 32);
        right.Children.Add(_queueButton);
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
