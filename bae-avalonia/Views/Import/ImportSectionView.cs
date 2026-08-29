using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The import triage sidebar: three tabs (Pending / Done / Skipped) over core's
// paged list, shown in the shell's content area when the Library/Import
// switcher selects Import (the macOS import sidebar's Avalonia counterpart —
// ImportCandidateListContent + TriageRowView share this partial view owner
// while their row rendering lives in separate files). Which items exist, in
// what order, under which header, in which tab, and the counts beside each tab
// are all core's, and arrive one window at a time; this view iterates and
// renders — it decides nothing about where a row belongs.
//
// A row click kicks off auto-identify for a still-queued candidate (there is
// nothing to map until it has been looked at) and otherwise puts the row under
// the mapping pane to the right — the release header, the file roles, the track
// slots, and the commit bar for that one folder. The folder-name mono subtitle
// macOS reserves for a focused row rides this row's tooltip instead.
//
// Built once and kept for the window's lifetime (the store subscription lives
// that long); entering the section resets to Pending and refreshes.
internal sealed partial class ImportSectionView : UserControl
{
    // Wide enough for the three tab labels and their count badges on one line.
    private const double PaneWidth = 420;

    private readonly AppService _app;
    private readonly ImportStore _import;
    private readonly StorageStore _storage;
    private readonly ImportMappingPane _pane;

    // The candidate the pane is holding, so a re-render of the list can accent
    // its row without asking the pane.
    private string? _selectedKey;

    // A selection whose row has not been read yet: the go-to-unidentified line
    // names a key that may sit outside every loaded window, so the pane opens
    // on it once its own query answers.
    private string? _pendingSelection;

    // The importable covers already handed to the image store, so an unchanged
    // queue does not enqueue the same decodes again.
    private List<string> _warmedReadyCovers = new();

    // The list itself, built once: a tab, filter, order or disclosure change
    // travels to core in the view request, and the same list re-ingests at the
    // same offsets rather than being rebuilt here.
    private readonly IncrementalListView<BridgeImportListItem> _listView;
    private readonly Panel _footBarHost = new();

    // Header controls, built once and mutated in place on Render() so the
    // filter TextBox never loses focus or caret position while the user types.
    private readonly Panel _tabBarHost = new() { };
    private readonly TextBox _filterBox = new() { BorderThickness = new Thickness(0), Background = Brushes.Transparent };
    private readonly Button _clearFilterButton;
    private readonly Button _listMenuButton;

    // The mark on the menu's trigger when a watched root's scan failed, so
    // the failure is findable without opening the menu.
    private readonly PathIcon _listMenuWarning = Icons.Glyph(Icons.Warning, 10, "BaeDangerBrush");

    // The sweep's progress, in the filter row: a ring and how many candidates
    // it still has to reach, opening the line itself. Built once and updated
    // in place — a control rebuilt under an open flyout takes the flyout down
    // with it.
    private readonly Button _progressButton;
    private readonly Arc _progressRing = new()
    {
        Width = 12,
        Height = 12,
        StartAngle = -90,
        StrokeThickness = 2,
        StrokeLineCap = PenLineCap.Round,
    };
    private readonly TextBlock _progressRemaining = new() { FontSize = 11.5, VerticalAlignment = VerticalAlignment.Center };
    private readonly TextBlock _progressCount = new() { FontSize = 12 };
    private readonly Panel _progressBarHost = new();
    private readonly Button _progressLine;
    private string? _progressGoToKey;

    // Folder scans have no denominator. Their filter-row control stays
    // indeterminate and opens the per-root current-generation counts core
    // projected alongside the total.
    private readonly Button _scanProgressButton;
    private readonly TextBlock _scanProgressCount = new() { FontSize = 11.5, VerticalAlignment = VerticalAlignment.Center };
    private readonly StackPanel _scanProgressFolders = new() { Spacing = 8, Width = 240 };

    public ImportSectionView(AppService app, ImportDialogs dialogs)
    {
        _app = app;
        _import = app.ImportStore;
        _storage = app.StorageStore;
        _pane = new ImportMappingPane(app, dialogs);
        _pane.Cleared += () => { _selectedKey = null; _import.ClearObservedCandidate(); Render(); };
        _listView = new IncrementalListView<BridgeImportListItem>(
            () => _import.List,
            _import.Item,
            BuildListCell,
            () => _import.ListFailure ?? Loc.Chrome(
                _import.FilterText.Length > 0
                    ? "import.empty.no_matches"
                    : "import.empty.nothing_here"));
        // The candidate under the pane can stop being a scanned folder — a
        // decision reshaped it, or the scan dropped it — and its own query says
        // so by delivering nothing.
        _import.ObservedCandidateGone += () => { _selectedKey = null; _pane.Clear(); Render(); };
        _import.ListSwapped += () => _listView.Rebind();

        _filterBox.Watermark = Loc.Chrome("import.filter.placeholder");
        _filterBox.FontSize = 12.5;
        _filterBox.TextChanged += (_, _) => _import.SetFilterText(_filterBox.Text ?? string.Empty);

        // The "clear" glyph (a Material close mark) as a small borderless icon
        // button — this app has no PathIcon geometry for the disc/barcode/clock
        // glyphs the macOS row uses, so those render as plain chips/text below,
        // but a close mark is already in Icons' vocabulary.
        _clearFilterButton = Icons.IconButton(
            "M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z",
            11, "BaeTextSecondaryBrush", 22);
        _clearFilterButton.Click += (_, _) => { _filterBox.Text = string.Empty; _import.SetFilterText(string.Empty); };

        _progressLine = BuildProgressLine();
        _progressButton = BuildProgressButton();
        _scanProgressButton = BuildScanProgressButton();

        _listMenuButton = ChromeButton("⋯", 15, _listMenuWarning);
        _listMenuButton.Click += (_, _) => { _listMenuButton.Flyout = BuildListMenuFlyout(); _listMenuButton.Flyout.ShowAt(_listMenuButton); };
        Avalonia.Automation.AutomationProperties.SetName(_listMenuButton, Loc.Chrome("import.list_menu"));

        Content = BuildShell();

        _import.Changed += Render;
        _storage.Changed += Render;

        Render();
    }

    // Entering the import section (a switcher click or a folder-drop flow):
    // land on Pending — the tab whose rows are waiting on the user — with a
    // fresh read.
    public void OnEntered()
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        _import.SetActiveTab(BridgeTriageTab.Pending);
    }

    // ── Shell ─────────────────────────────────────────────────────────────────

    private Control BuildShell()
    {
        // The list fills the sidebar, with Pending's bulk-import bar under it.
        var listColumn = new Grid { RowDefinitions = new RowDefinitions("*,Auto") };
        Grid.SetRow(_listView, 0);
        Grid.SetRow(_footBarHost, 1);
        listColumn.Children.Add(_listView);
        listColumn.Children.Add(_footBarHost);

        var header = new StackPanel { Spacing = 0 };
        header.Children.Add(BuildTabBarRow());
        // The tabs choose what the list holds; the filter narrows what it
        // shows. Two jobs, so the header says where one ends.
        header.Children.Add(HeaderDivider());
        header.Children.Add(BuildFilterRow());
        var headerHost = new Border { Child = header };
        headerHost[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");

        var divider = HeaderDivider();

        var column = new Grid { RowDefinitions = new RowDefinitions("Auto,Auto,*") };
        Grid.SetRow(headerHost, 0);
        Grid.SetRow(divider, 1);
        Grid.SetRow(listColumn, 2);
        column.Children.Add(headerHost);
        column.Children.Add(divider);
        column.Children.Add(listColumn);

        var sidebar = new Border
        {
            Width = PaneWidth,
            VerticalAlignment = VerticalAlignment.Stretch,
            BorderThickness = new Thickness(0, 0, 1, 0),
            Child = column,
        };
        sidebar[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");
        sidebar[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeHairlineBrush");

        // The sidebar triages, the mapping pane maps: the two sit side by side,
        // and neither ever covers the other.
        var split = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*") };
        Grid.SetColumn(sidebar, 0);
        Grid.SetColumn(_pane, 1);
        split.Children.Add(sidebar);
        split.Children.Add(_pane);
        return split;
    }

    private static Border HeaderDivider()
    {
        var divider = new Border { Height = 1 };
        divider[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeHairlineBrush");
        return divider;
    }

    private Control BuildTabBarRow()
    {
        _tabBarHost.Margin = new Thickness(10, 10, 10, 10);
        return _tabBarHost;
    }

    private Control BuildFilterRow()
    {
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto,Auto,Auto,Auto"), ColumnSpacing = 6 };
        var searchGlyph = Icons.Glyph(Icons.Search, 13, "BaeTextSecondaryBrush");
        Grid.SetColumn(searchGlyph, 0);

        Grid.SetColumn(_filterBox, 1);

        Grid.SetColumn(_clearFilterButton, 2);

        Grid.SetColumn(_scanProgressButton, 3);

        Grid.SetColumn(_progressButton, 4);

        Grid.SetColumn(_listMenuButton, 5);

        row.Children.Add(searchGlyph);
        row.Children.Add(_filterBox);
        row.Children.Add(_clearFilterButton);
        row.Children.Add(_scanProgressButton);
        row.Children.Add(_progressButton);
        row.Children.Add(_listMenuButton);

        return new Border { Padding = new Thickness(14, 9, 10, 9), Child = row };
    }

    // The compact indicator: a ring at the sweep's fraction and how many
    // candidates it still has to reach. The ring is a glance — the numbers
    // are in the flyout it opens.
    private Button BuildProgressButton()
    {
        var track = new Ellipse
        {
            Width = 12,
            Height = 12,
            StrokeThickness = 2,
            Opacity = 0.25,
        };
        track[!Shape.StrokeProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        _progressRing[!Shape.StrokeProperty] = new DynamicResourceExtension("BaeAccentBrush");
        var ring = new Panel { Width = 12, Height = 12, VerticalAlignment = VerticalAlignment.Center };
        ring.Children.Add(track);
        ring.Children.Add(_progressRing);

        _progressRemaining[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var content = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 4,
            Children = { ring, _progressRemaining },
        };
        var button = new Button
        {
            Content = content,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4),
            Flyout = new Flyout { Content = _progressLine },
        };
        ToolTip.SetTip(button, Loc.Chrome("import.progress.identifying"));
        return button;
    }

    private Button BuildScanProgressButton()
    {
        _scanProgressCount[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        var content = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 4,
            Children =
            {
                new Spinner { Width = 12, Height = 12 },
                _scanProgressCount,
            },
        };
        var button = new Button
        {
            Content = content,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4),
            Flyout = new Flyout
            {
                Content = new Border
                {
                    Padding = new Thickness(12),
                    Child = _scanProgressFolders,
                },
            },
        };
        var label = Loc.Core("ui.import.scan.activity");
        ToolTip.SetTip(button, label);
        Avalonia.Automation.AutomationProperties.SetName(button, label);
        return button;
    }

    // The line the indicator opens: "Identifying N / total" over a thin bar.
    // It is a control, not a label. The candidates the count is waiting on are
    // rows somewhere in the queue, and a number that sits still while giving
    // no way to reach what it is waiting on is the frustrating half of this
    // pane. Clicking it goes to the first.
    private Button BuildProgressLine()
    {
        var label = new TextBlock { Text = Loc.Chrome("import.progress.identifying"), FontSize = 12 };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        _progressCount[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var labelRow = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto") };
        Grid.SetColumn(label, 0);
        Grid.SetColumn(_progressCount, 1);
        labelRow.Children.Add(label);
        labelRow.Children.Add(_progressCount);

        var line = new Button
        {
            Content = new StackPanel
            {
                Spacing = 7,
                Width = 200,
                Children = { labelRow, _progressBarHost },
            },
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        };
        ToolTip.SetTip(line, Loc.Chrome("import.progress.go_to_unidentified"));
        line.Click += (_, _) =>
        {
            if (_progressGoToKey is not { } key)
            {
                return;
            }
            _progressButton.Flyout?.Hide();
            _import.SetActiveTab(BridgeTriageTab.Pending);
            SelectCandidate(key);
        };
        return line;
    }

    // A borderless text button for the filter row's sort/add-folder controls —
    // a symbol at `fontSize`, secondary-colored, no background until pressed,
    // with an optional mark beside the symbol.
    private static Button ChromeButton(string symbol, double fontSize, Control? mark = null)
    {
        var symbolText = new TextBlock { Text = symbol, FontSize = fontSize, VerticalAlignment = VerticalAlignment.Center };
        var content = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 2,
            Children = { symbolText },
        };
        if (mark is not null)
        {
            content.Children.Add(mark);
        }
        var button = new Button
        {
            Content = content,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(4),
        };
        button[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return button;
    }

    // The candidate list's one menu: everything that acts on the list rather
    // than on a row — the sort order, and the watched folders the list is built
    // from. One control rather than two, since a separate "+" advertised adding
    // a folder as a peer of sorting when it is one item among the roots already
    // being watched.
    private MenuFlyout BuildListMenuFlyout()
    {
        var items = new List<Control>();

        // The name order decides the tabs that are ordered by name. Done is
        // ordered by what the cloud is still doing with each release and when
        // it was imported, so on that tab there is nothing here to choose.
        if (_import.ActiveTab != BridgeTriageTab.Done)
        {
            var az = new MenuItem
            {
                Header = (_import.SortOrder == CandidateSortOrder.NameAZ ? "✓ " : string.Empty) + Loc.Chrome("import.sort.name_az"),
            };
            az.Click += (_, _) => _import.SetSortOrder(CandidateSortOrder.NameAZ);
            var za = new MenuItem
            {
                Header = (_import.SortOrder == CandidateSortOrder.NameZA ? "✓ " : string.Empty) + Loc.Chrome("import.sort.name_za"),
            };
            za.Click += (_, _) => _import.SetSortOrder(CandidateSortOrder.NameZA);
            items.Add(az);
            items.Add(za);
            items.Add(new Separator());
        }

        var add = new MenuItem { Header = Loc.Chrome("import.folder.add") };
        add.Click += async (_, _) => await AddFolder();
        items.Add(add);

        var scans = _import.Summary.FolderScanStatuses
            .ToDictionary(scan => scan.WatchedFolderPath, scan => scan.Status);
        // The roots on a volume served over the network. Such a folder is
        // checked on a schedule rather than reported the moment it changes, so
        // its entry says so — an album added on the server that has not
        // appeared yet is otherwise a mystery.
        var onNetwork = _import.Summary.FolderScanStatuses
            .Where(scan => scan.OnNetworkVolume)
            .Select(scan => scan.WatchedFolderPath)
            .ToHashSet();
        var networkLine = Loc.Core(
            BaeBridgeMethods.BridgeNetworkFolderWatchKey(),
            "minutes",
            (long)BaeBridgeMethods.BridgeNetworkFolderCheckMinutes());
        foreach (var folder in _import.WatchedFolders)
        {
            var path = folder.Path;
            var folderItem = new MenuItem { Header = folder.Name };
            // A root being walked, or one whose walk failed, says so on its
            // own entry — a failure carries what went wrong as its tooltip,
            // which is where the header used to spell it out.
            var network = onNetwork.Contains(path);
            switch (scans.GetValueOrDefault(path))
            {
                case BridgeFolderScanStatus.Scanning:
                    folderItem.Icon = new Spinner { Width = 12, Height = 12 };
                    break;
                case BridgeFolderScanStatus.Failed failed:
                    folderItem.Icon = Icons.Glyph(Icons.Warning, 12, "BaeDangerBrush");
                    ToolTip.SetTip(
                        folderItem,
                        network ? $"{failed.Error}\n{networkLine}" : failed.Error);
                    break;
                default:
                    if (network)
                    {
                        folderItem.Icon = Icons.Glyph(
                            Icons.NetworkFolder, 12, "BaeTextSecondaryBrush");
                    }
                    break;
            }
            if (network && scans.GetValueOrDefault(path) is not BridgeFolderScanStatus.Failed)
            {
                ToolTip.SetTip(folderItem, networkLine);
            }
            var refreshing = _import.Interaction.IsRefreshing(path);
            var refresh = new MenuItem
            {
                Header = Loc.Chrome(
                    refreshing
                        ? "import.folder.refreshing"
                        : "import.folder.refresh"),
                IsEnabled = !refreshing,
            };
            refresh.Click += (_, _) => _import.RefreshWatchedFolder(path);
            var reveal = new MenuItem { Header = Loc.Chrome("libraries.reveal") };
            reveal.Click += (_, _) => RevealInFileManager.Reveal(path);
            var remove = new MenuItem { Header = Loc.Chrome("import.folder.remove") };
            remove.Click += (_, _) => _import.RemoveWatchedFolder(path);
            folderItem.Items.Add(refresh);
            folderItem.Items.Add(reveal);
            folderItem.Items.Add(new Separator());
            folderItem.Items.Add(remove);
            items.Add(folderItem);
        }

        return new MenuFlyout { ItemsSource = items };
    }

    private async Task AddFolder()
    {
        var storage = TopLevel.GetTopLevel(this)?.StorageProvider;
        if (storage is null)
        {
            return;
        }
        var folders = await storage.OpenFolderPickerAsync(new FolderPickerOpenOptions { AllowMultiple = false });
        if (folders.Count == 0 || folders[0].TryGetLocalPath() is not { } path)
        {
            return;
        }
        var (current, error) = await _import.ScanFolder(path);
        if (current && error is not null)
        {
            _app.ShowError(Loc.Chrome("import.error_title"), error);
        }
    }

    // ── Render ────────────────────────────────────────────────────────────────

    // Rebuilds the tab bar, the progress line and the foot bar on every
    // ImportStore.Changed tick, and tells the realized rows to render again.
    // The list itself is not rebuilt: its rows arrive from core one window at a
    // time, and rebuilding it here would drop the scroll position with them.
    // The filter TextBox is never touched here — it is the source of truth for
    // what the user typed, not a mirror of it.
    private void Render()
    {
        RenderTabBar();
        _clearFilterButton.IsVisible = _import.FilterText.Length > 0;
        RenderProgressIndicator();
        RenderScanProgressIndicator();
        _listMenuWarning.IsVisible = _import.Summary.FolderScanStatuses
            .Any(scan => scan.Status is BridgeFolderScanStatus.Failed);
        RenderFootBar();
        ShowPendingSelection();
        _listView.Refresh();
        // A row's own content can change without the ordering changing — a
        // checkbox toggled, an import ticking — so the realized rows render
        // again in place rather than the list being rebuilt under them.
        _import.List.NotifyRowsChanged();
        WarmReadyCovers();
    }

    // Importable rows are covers this app has already downloaded once.
    // Decoding them as the queue lands keeps Pending's first paint from being
    // a grid of blanks. Keyed on the URL list so an unchanged queue warms
    // nothing.
    private void WarmReadyCovers()
    {
        var urls = _import.Summary.Ready
            .Select(row => row.CoverThumbnailUrl)
            .Where(url => !string.IsNullOrEmpty(url))
            .Select(url => url!)
            .ToList();
        if (urls.SequenceEqual(_warmedReadyCovers))
        {
            return;
        }
        _warmedReadyCovers = urls;
        _ = _app.Images.WarmAsync(
            urls.Select(url => (ImageContent)new ImageContent.Remote(url)),
            ImageWidths.Row);
    }

    private void RenderTabBar()
    {
        _tabBarHost.Children.Clear();
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*,*"), ColumnSpacing = 4 };
        var counts = _import.Summary.Counts;
        AddTabSegment(row, 0, BridgeTriageTab.Pending, Loc.Chrome("import.tab.pending"), counts.Pending);
        AddTabSegment(row, 1, BridgeTriageTab.Done, Loc.Chrome("import.tab.done"), counts.Done);
        AddTabSegment(row, 2, BridgeTriageTab.Skipped, Loc.Chrome("import.tab.skipped"), counts.Skipped);
        _tabBarHost.Children.Add(row);
    }

    private void AddTabSegment(Grid row, int column, BridgeTriageTab tab, string label, uint count)
    {
        var isActive = _import.ActiveTab == tab;
        var text = new TextBlock { Text = label, FontSize = 12.5, FontWeight = FontWeight.SemiBold, HorizontalAlignment = HorizontalAlignment.Center };
        var badge = new Border
        {
            CornerRadius = new CornerRadius(999),
            Padding = new Thickness(6, 1),
            Margin = new Thickness(4, 0, 0, 0),
            Child = new TextBlock { Text = count.ToString(CultureInfo.CurrentCulture), FontSize = 11, FontWeight = FontWeight.SemiBold },
        };
        badge[!Border.BackgroundProperty] = new DynamicResourceExtension(isActive ? "BaeAccentBrush" : "BaeElevatedBrush");
        var content = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Center, Children = { text, badge } };

        var segment = new Border
        {
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(9, 5),
            Child = content,
        };
        if (isActive)
        {
            segment[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        else
        {
            segment.Background = Brushes.Transparent;
        }
        var foreground = isActive ? "BaeAccentBrush" : "BaeTextSecondaryBrush";
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(foreground);
        var badgeText = (TextBlock)badge.Child!;
        badgeText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(isActive ? "BaeOnAccentBrush" : "BaeTextSecondaryBrush");

        var button = new Button
        {
            Content = segment,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
        };
        button.Click += (_, _) => _import.SetActiveTab(tab);
        Grid.SetColumn(button, column);
        row.Children.Add(button);
    }

    // The sweep's progress, updated in place: the indicator leaves the filter
    // row when there is nothing left to identify, and the line inside its
    // flyout is kept current so an open flyout does not freeze mid-sweep.
    private void RenderProgressIndicator()
    {
        if (_import.QueueIdentifyProgress is not { } progress
            || progress.Total == 0
            || progress.Identified >= progress.Total)
        {
            _progressButton.IsVisible = false;
            _progressGoToKey = null;
            return;
        }
        _progressButton.IsVisible = true;
        var fraction = (double)progress.Identified / progress.Total;
        _progressRing.SweepAngle = 360 * fraction;
        // The same two numbers beside the ring and inside the flyout it opens,
        // so the glance and the line never disagree.
        var counted =
            $"{progress.Identified.ToString(CultureInfo.CurrentCulture)} / {progress.Total.ToString(CultureInfo.CurrentCulture)}";
        _progressRemaining.Text = counted;
        _progressCount.Text = counted;
        _progressBarHost.Children.Clear();
        _progressBarHost.Children.Add(ImportProgressLine.Bar(fraction));
        _progressGoToKey = _import.Summary.FirstUnidentified?.CandidateKey;
        _progressLine.IsEnabled = _progressGoToKey is not null;
    }

    private void RenderScanProgressIndicator()
    {
        if (_import.Summary.FolderScanActivity is not { } activity)
        {
            _scanProgressButton.IsVisible = false;
            _scanProgressFolders.Children.Clear();
            return;
        }

        _scanProgressButton.IsVisible = true;
        _scanProgressCount.Text = Loc.Core(
            "ui.import.scan.found",
            "count",
            (long)activity.FoundCount);
        _scanProgressFolders.Children.Clear();
        foreach (var folder in activity.Folders)
        {
            var row = new Grid
            {
                ColumnDefinitions = new ColumnDefinitions("*,Auto"),
                ColumnSpacing = 12,
            };
            var name = new TextBlock
            {
                Text = folder.WatchedFolderName,
                TextTrimming = TextTrimming.CharacterEllipsis,
            };
            var count = new TextBlock
            {
                Text = Loc.Core(
                    "ui.import.scan.found",
                    "count",
                    (long)folder.FoundCount),
            };
            count[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            Grid.SetColumn(name, 0);
            Grid.SetColumn(count, 1);
            row.Children.Add(name);
            row.Children.Add(count);
            _scanProgressFolders.Children.Add(row);
        }
    }

    // ── The foot bar ─────────────────────────────────────────────────────────

    // Pending's bulk-import bar, over the Ready set core computed for the view
    // the list is showing. The other two tabs have nothing to act on in bulk.
    private void RenderFootBar()
    {
        _footBarHost.Children.Clear();
        if (_import.ActiveTab is not BridgeTriageTab.Pending)
        {
            return;
        }
        var readyKeys = _import.Summary.Ready.Select(row => row.CandidateKey).ToList();
        var selectedCount = _import.SelectedReady.Count(readyKeys.Contains);
        _footBarHost.Children.Add(BuildFootBar(selectedCount, readyKeys));
    }

    private Control BuildFootBar(int selectedCount, List<string> readyKeys)
    {
        var selectedText = new TextBlock
        {
            Text = Loc.Chrome("import.footbar.selected", "count", (long)selectedCount),
            FontSize = 12.5,
            VerticalAlignment = VerticalAlignment.Center,
        };
        selectedText[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        // Every selectable row already selected leaves the control nothing to
        // add, so it becomes the way to clear.
        var allSelected = readyKeys.Count > 0 && selectedCount >= readyKeys.Count;
        var selectAll = new Button
        {
            Content = Loc.Chrome(
                allSelected ? "import.footbar.select_none" : "import.footbar.select_all"),
            Padding = new Thickness(13, 6),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(1),
            Background = Brushes.Transparent,
            IsEnabled = readyKeys.Count > 0,
        };
        selectAll[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        selectAll[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        selectAll.Click += (_, _) =>
        {
            if (allSelected)
            {
                _import.ClearReadySelection();
            }
            else
            {
                _import.SelectAllReady(readyKeys);
            }
        };

        var import = new Button
        {
            Content = Loc.Chrome("import.footbar.import_count", "count", (long)selectedCount),
            Padding = new Thickness(15, 6),
            CornerRadius = new CornerRadius(999),
            IsEnabled = selectedCount > 0,
            Opacity = selectedCount == 0 ? 0.5 : 1,
        };
        import.Classes.Add("accent");
        import.Click += async (_, _) => await ImportSelectedReady();

        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto,Auto"), ColumnSpacing = 8 };
        Grid.SetColumn(selectedText, 0);
        Grid.SetColumn(selectAll, 1);
        Grid.SetColumn(import, 2);
        row.Children.Add(selectedText);
        row.Children.Add(selectAll);
        row.Children.Add(import);

        var bar = new Border { Padding = new Thickness(12, 11), Child = row };
        bar[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        return bar;
    }

    // Import every selected Ready candidate — N existing single imports,
    // dispatched independently; there is no batch primitive in core. A bulk
    // import never opens the confirm form: it commits the prefetch's unedited
    // defaults straight onto the row's matched release, which is exactly what
    // the Ready rule already guarantees is safe (one confident match, not in
    // the library, counts and lengths agree).
    private async Task ImportSelectedReady()
    {
        var ready = _import.Summary.Ready.ToList();
        var readyKeys = ready.Select(row => row.CandidateKey).ToHashSet();
        var keys = _import.SelectedReady.Where(readyKeys.Contains).ToList();
        if (keys.Count == 0)
        {
            return;
        }

        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        if (!settingsCurrent)
        {
            return;
        }
        var storageMode = settings.HasCloudHome ? "cloud" : "local";
        var pinned = settings.HasCloudHome;

        var failureCount = 0;
        foreach (var key in keys)
        {
            // Selection can outlive the row that earned it (imported by a
            // faster sibling call, or reclassified) — the foot bar already
            // intersects the selection against the current Ready set, so a miss
            // here is defensive, not expected.
            if (ready.FirstOrDefault(row => row.CandidateKey == key) is null)
            {
                continue;
            }

            // A Ready row's decision is already stored — its settled single
            // match — along with everything else the pane would have shown, so
            // committing without opening it writes exactly the same release.
            var (importCurrent, error) = await _app.Import.CommitImport(
                key, storageMode, pinned);
            if (!importCurrent)
            {
                return;
            }
            if (error is not null)
            {
                failureCount++;
            }
        }

        _import.ClearReadySelection();
        if (failureCount > 0)
        {
            _app.ShowError(
                Loc.Chrome("import.error_title"),
                Loc.Chrome("import.bulk_import_failed", "count", (long)failureCount));
        }
    }
}
