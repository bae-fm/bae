using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The library shell (desktop story 3 in its empty state): the chrome around an
// open library. Across the top a Library/Import switcher and a search field;
// below, either the library browser (a large bold mode heading that is itself a
// mode dropdown with sort controls opposite, over the grid) or the import
// section, swapped by the switcher; a docked queue sidebar; and a now-playing
// bar along the bottom. Every color reads a theme brush, so the shell
// renders in either OS appearance.
internal sealed class MainShellView : UserControl, System.IDisposable
{
    private readonly AppService _app;
    private readonly LibraryBrowserView _browser;
    private readonly ImportSectionView _importSection;
    private readonly QueuePane _queuePane;
    private readonly ArtworkLoadingBanner _artworkLoadingBanner;
    private readonly Button _queueButton;

    // The two switcher segments, restyled as the active section changes.
    private Button _librarySegment = null!;
    private TextBlock _libraryLabel = null!;
    private Button _importSegment = null!;
    private TextBlock _importLabel = null!;

    public MainShellView(
        AppService app,
        PlaybackCommands playback,
        ReleaseActionDialogs dialogs,
        ImportDialogs importDialogs)
    {
        _app = app;
        _queueButton = Icons.IconButton(Icons.Queue, 17, "BaeTextSecondaryBrush", 32);
        Avalonia.Automation.AutomationProperties.SetName(_queueButton, Loc.Chrome("queue.title"));
        ToolTip.SetTip(_queueButton, Loc.Chrome("queue.title"));

        var root = new Grid { RowDefinitions = new RowDefinitions("Auto,Auto,*,Auto") };
        SetBg(root, "BaeBackgroundBrush");

        var toolbar = BuildToolbar();
        Grid.SetRow(toolbar, 0);
        root.Children.Add(toolbar);

        _artworkLoadingBanner = new ArtworkLoadingBanner(_app.ArtworkLoadingStore);
        Grid.SetRow(_artworkLoadingBanner, 1);
        root.Children.Add(_artworkLoadingBanner);

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
        Grid.SetRow(contentRow, 2);
        root.Children.Add(contentRow);

        // The now-playing bar reads persisted preferences, so the shell — not just
        // the settings window — keeps the settings mirror current for the window's
        // life. Seeded before the bar is built so its controls start correct.
        _app.SettingsStore.Reload();

        var bar = new NowPlayingBar(
            _app.PlaybackStore,
            _app.SettingsStore,
            playback,
            _app.Images,
            albumId => _ = OpenAlbum(albumId),
            new CastButton(_app.CastStore, _app.SettingsStore),
            _queueButton);
        Grid.SetRow(bar, 3);
        root.Children.Add(bar);

        Content = root;

        _queuePane = new QueuePane(_app, queueHost);
        _queuePane.AttachToggle(_queueButton);
    }

    public void Dispose()
    {
        _artworkLoadingBanner.Dispose();
        _queuePane.Dispose();
    }

    // Switch to the import section and land it on a fresh Pending tab — the
    // switcher click, and the folder-drop / activation flows that route here.
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
        // active segment resting on a neutral tile.
        var pill = new Border
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            CornerRadius = new CornerRadius(11),
            Padding = new Thickness(4),
            BorderThickness = new Thickness(1),
        };
        SetBg(pill, "BaeWellBrush");
        SetBorder(pill, "BaeHairlineBrush");
        var segments = new StackPanel { Orientation = Orientation.Horizontal };
        (_librarySegment, _libraryLabel) = BuildSegment(Loc.Chrome("section.library"));
        (_importSegment, _importLabel) = BuildSegment(Loc.Chrome("section.import"));
        _librarySegment.Click += (_, _) => ShowLibrary();
        _importSegment.Click += (_, _) => ShowImport();
        segments.Children.Add(_librarySegment);
        segments.Children.Add(_importSegment);
        pill.Child = segments;
        grid.Children.Add(pill);
        SetActiveSection(import: false);

        // Right cluster: the search field. The library and playback commands
        // live in the window's menu bar.
        var right = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 12,
            HorizontalAlignment = HorizontalAlignment.Right,
            VerticalAlignment = VerticalAlignment.Center,
        };
        right.Children.Add(BuildSearchField());
        grid.Children.Add(right);

        strip.Child = grid;
        return strip;
    }

    private static (Button Segment, TextBlock Label) BuildSegment(string text)
    {
        var segment = new Button
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(20, 7),
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
        };
        segment.Classes.Add("navigation");
        var label = new TextBlock
        {
            Text = text,
            FontSize = 14.5,
            FontWeight = FontWeight.Bold,
        };
        segment.Content = label;
        return (segment, label);
    }

    // Give the active segment a neutral tile and de-emphasize the other, matching
    // the visible section.
    private void SetActiveSection(bool import)
    {
        StyleSegment(_librarySegment, _libraryLabel, active: !import);
        StyleSegment(_importSegment, _importLabel, active: import);
    }

    private static void StyleSegment(Button segment, TextBlock label, bool active)
    {
        if (active)
        {
            segment[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeTileBrush");
            label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
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

    private static void SetBg(Border border, string key) =>
        border[!Border.BackgroundProperty] = new DynamicResourceExtension(key);

    private static void SetBg(Panel panel, string key) =>
        panel[!Panel.BackgroundProperty] = new DynamicResourceExtension(key);

    private static void SetBorder(Border border, string key) =>
        border[!Border.BorderBrushProperty] = new DynamicResourceExtension(key);
}
