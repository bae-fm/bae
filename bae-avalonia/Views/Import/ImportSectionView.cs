using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
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
    private readonly Panel _progressHost = new();

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
            () => Loc.Chrome(
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

        _listMenuButton = ChromeButton("⋯", 15);
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
        header.Children.Add(BuildFilterRow());
        header.Children.Add(_progressHost);
        var headerHost = new Border { Child = header };
        headerHost[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSurfaceBrush");

        var divider = new Border { Height = 1 };
        divider[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeHairlineBrush");

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

    private Control BuildTabBarRow()
    {
        _tabBarHost.Margin = new Thickness(10, 10, 10, 8);
        return _tabBarHost;
    }

    private Control BuildFilterRow()
    {
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto,Auto"), ColumnSpacing = 6 };
        var searchGlyph = Icons.Glyph(Icons.Search, 13, "BaeTextSecondaryBrush");
        Grid.SetColumn(searchGlyph, 0);

        Grid.SetColumn(_filterBox, 1);

        Grid.SetColumn(_clearFilterButton, 2);

        Grid.SetColumn(_listMenuButton, 3);

        row.Children.Add(searchGlyph);
        row.Children.Add(_filterBox);
        row.Children.Add(_clearFilterButton);
        row.Children.Add(_listMenuButton);

        return new Border { Padding = new Thickness(14, 0, 10, 10), Child = row };
    }

    // A borderless text button for the filter row's sort/add-folder controls —
    // a symbol at `fontSize`, secondary-colored, no background until pressed.
    private static Button ChromeButton(string symbol, double fontSize)
    {
        var button = new Button
        {
            Content = new TextBlock { Text = symbol, FontSize = fontSize, VerticalAlignment = VerticalAlignment.Center },
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

        var items = new List<Control> { az, za, new Separator() };

        var add = new MenuItem { Header = Loc.Chrome("import.folder.add") };
        add.Click += async (_, _) => await AddFolder();
        items.Add(add);

        foreach (var folder in _import.WatchedFolders)
        {
            var path = folder.Path;
            var folderItem = new MenuItem { Header = folder.Name };
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
        RenderProgress();
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

    private void RenderProgress()
    {
        _progressHost.Children.Clear();
        var column = new StackPanel
        {
            Spacing = 7,
            Margin = new Thickness(14, 0, 14, 12),
        };
        foreach (var scan in _import.Summary.FolderScanStatuses)
        {
            var prefix = $"{scan.WatchedFolderName} ({scan.WatchedFolderPath})";
            var text = scan.Status switch
            {
                BridgeFolderScanStatus.Scanning =>
                    $"{prefix}: {Loc.Chrome("import.scanning")}",
                BridgeFolderScanStatus.Failed failed =>
                    $"{prefix}: {failed.Error}",
                _ => null,
            };
            if (text is null)
            {
                continue;
            }
            var status = new TextBlock
            {
                Text = text,
                FontSize = 11.5,
                MaxLines = 2,
                TextTrimming = TextTrimming.CharacterEllipsis,
            };
            status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(
                scan.Status is BridgeFolderScanStatus.Failed
                    ? "BaeDangerBrush"
                    : "BaeTextSecondaryBrush");
            var refreshing = _import.Interaction.IsRefreshing(scan.WatchedFolderPath);
            var refresh = new Button
            {
                Content = Loc.Chrome(
                    refreshing
                        ? "import.folder.refreshing"
                        : "import.folder.refresh"),
                IsEnabled = !refreshing,
                Padding = new Thickness(8, 3),
            };
            refresh.Click += (_, _) =>
                _import.RefreshWatchedFolder(scan.WatchedFolderPath);
            var statusRow = new Grid
            {
                ColumnDefinitions = new ColumnDefinitions("*,Auto"),
                ColumnSpacing = 8,
            };
            Grid.SetColumn(status, 0);
            Grid.SetColumn(refresh, 1);
            statusRow.Children.Add(status);
            statusRow.Children.Add(refresh);
            column.Children.Add(statusRow);
        }
        if (_import.QueueIdentifyProgress is { } progress && progress.Total > 0)
        {
            var label = new TextBlock { Text = Loc.Chrome("import.progress.identifying"), FontSize = 12 };
            label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            var count = new TextBlock
            {
                Text = $"{progress.Identified.ToString(CultureInfo.CurrentCulture)} / {progress.Total.ToString(CultureInfo.CurrentCulture)}",
                FontSize = 12,
            };
            count[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            var labelRow = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto") };
            Grid.SetColumn(label, 0);
            Grid.SetColumn(count, 1);
            labelRow.Children.Add(label);
            labelRow.Children.Add(count);

            // The line is a control, not a label. The candidates the count is
            // waiting on are rows somewhere in the queue, and a number that
            // sits still while giving no way to reach what it is waiting on is
            // the frustrating half of this pane. Clicking it goes to the first.
            var unidentified = _import.Summary.FirstUnidentifiedKey;
            var lineContent = new StackPanel
            {
                Spacing = 7,
                Children =
                {
                    labelRow,
                    ThinProgressBar((double)progress.Identified / progress.Total),
                },
            };
            var line = new Button
            {
                Content = lineContent,
                Background = Brushes.Transparent,
                BorderThickness = new Thickness(0),
                Padding = new Thickness(0),
                HorizontalAlignment = HorizontalAlignment.Stretch,
                HorizontalContentAlignment = HorizontalAlignment.Stretch,
                IsEnabled = unidentified is not null,
            };
            ToolTip.SetTip(line, Loc.Chrome("import.progress.go_to_unidentified"));
            if (unidentified is { } key)
            {
                line.Click += (_, _) =>
                {
                    _import.SetActiveTab(BridgeTriageTab.Pending);
                    SelectCandidate(key);
                };
            }
            column.Children.Add(line);
        }
        if (column.Children.Count > 0)
        {
            _progressHost.Children.Add(column);
        }
    }

    private static Control ThinProgressBar(double fraction)
    {
        var clamped = Math.Clamp(fraction, 0, 1);
        var fill = new ColumnDefinition { Width = new GridLength(clamped, GridUnitType.Star) };
        var rest = new ColumnDefinition { Width = new GridLength(1 - clamped, GridUnitType.Star) };
        var fillBar = new Border { CornerRadius = new CornerRadius(1.5) };
        fillBar[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        var track = new Grid { ColumnDefinitions = new ColumnDefinitions { fill, rest } };
        Grid.SetColumn(fillBar, 0);
        track.Children.Add(fillBar);
        return new Border { Height = 3, CornerRadius = new CornerRadius(1.5), Child = track, ClipToBounds = true };
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

        var selectAll = new Button
        {
            Content = Loc.Chrome("import.footbar.select_all"),
            Padding = new Thickness(13, 6),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(1),
            Background = Brushes.Transparent,
            IsEnabled = readyKeys.Count > 0,
        };
        selectAll[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        selectAll[!Button.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        selectAll.Click += (_, _) => _import.SelectAllReady(readyKeys);

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
