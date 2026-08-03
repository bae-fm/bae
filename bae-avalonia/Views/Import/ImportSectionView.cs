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

// The import triage sidebar: four tabs (Ready / Needs You / Done / Skipped) over
// core's BridgeTriageQueue projection, shown in the shell's content area when the
// Library/Import switcher selects Import (the macOS import sidebar's Avalonia
// counterpart — ImportCandidateListContent + TriageRowView collapsed into one
// file, matching this codebase's per-feature single-file convention, e.g.
// QueuePane). Every row, its tab, its Needs-you group, and the tab counts come
// from ImportStore.TriageQueue, read through TriageListModel's
// filter/group/sort; this view iterates and renders — it decides nothing about
// where a row belongs.
//
// A row click kicks off auto-identify for a still-queued candidate (there is
// nothing to map until it has been looked at) and otherwise puts the row under
// the mapping pane to the right — the release header, the file roles, the track
// slots, and the commit bar for that one folder. The folder-name mono subtitle
// macOS reserves for a focused row rides this row's tooltip instead.
//
// Built once and kept for the window's lifetime (the store subscription lives
// that long); entering the section resets to Ready and refreshes.
internal sealed class ImportSectionView : UserControl
{
    // Wide enough for the four tab labels and their count badges on one line.
    private const double PaneWidth = 420;

    private readonly AppService _app;
    private readonly ImportStore _import;
    private readonly ImportMappingPane _pane;

    // The candidate the pane is holding, so a re-render of the list can accent
    // its row without asking the pane.
    private string? _selectedKey;

    // Header controls, built once and mutated in place on Render() so the
    // filter TextBox never loses focus or caret position while the user types.
    private readonly Panel _tabBarHost = new() { };
    private readonly TextBox _filterBox = new() { BorderThickness = new Thickness(0), Background = Brushes.Transparent };
    private readonly Button _clearFilterButton;
    private readonly Button _sortButton;
    private readonly Button _folderMenuButton;
    private readonly Panel _progressHost = new();
    private readonly ContentControl _contentSlot = new()
    {
        HorizontalContentAlignment = HorizontalAlignment.Stretch,
        VerticalContentAlignment = VerticalAlignment.Stretch,
    };

    public ImportSectionView(AppService app, ImportDialogs dialogs)
    {
        _app = app;
        _import = app.ImportStore;
        _pane = new ImportMappingPane(app, dialogs);
        _pane.Cleared += () => { _selectedKey = null; Render(); };

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

        _sortButton = ChromeButton("⋯", 15);
        _sortButton.Click += (_, _) => { _sortButton.Flyout = BuildSortFlyout(); _sortButton.Flyout.ShowAt(_sortButton); };
        Avalonia.Automation.AutomationProperties.SetName(_sortButton, Loc.Chrome("import.sort_by"));

        _folderMenuButton = ChromeButton("+", 15);
        _folderMenuButton.Click += (_, _) => { _folderMenuButton.Flyout = BuildFolderFlyout(); _folderMenuButton.Flyout.ShowAt(_folderMenuButton); };
        Avalonia.Automation.AutomationProperties.SetName(_folderMenuButton, Loc.Chrome("import.folder.add"));

        Content = BuildShell();

        _import.Changed += Render;

        // Re-read on every import-domain invalidation the triage queue reads
        // from. Registered once for the window's lifetime, matching how
        // LibraryBrowserView registers its own store's projections.
        app.ProjectionRegistry.Register(typeof(BridgeInvalidation.ImportCandidateList), _import.RefreshTriageQueue);
        app.ProjectionRegistry.Register(typeof(BridgeInvalidation.ImportCandidate), _import.RefreshTriageQueue);
        app.ProjectionRegistry.Register(typeof(BridgeInvalidation.WatchedFolders), _import.RefreshTriageQueue);
        app.ProjectionRegistry.Register(typeof(BridgeInvalidation.Release), _import.RefreshTriageQueue);

        Render();
    }

    // Entering the import section (a switcher click or a folder-drop flow):
    // land on Needs You — the tab whose rows are waiting on the user — with a
    // fresh read.
    public void OnEntered()
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        _import.SetActiveTab(BridgeTriageTab.NeedsYou);
        _import.RefreshTriageQueue();
    }

    // ── Shell ─────────────────────────────────────────────────────────────────

    private Control BuildShell()
    {
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
        Grid.SetRow(_contentSlot, 2);
        column.Children.Add(headerHost);
        column.Children.Add(divider);
        column.Children.Add(_contentSlot);

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
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("Auto,*,Auto,Auto,Auto"), ColumnSpacing = 6 };
        var searchGlyph = Icons.Glyph(Icons.Search, 13, "BaeTextSecondaryBrush");
        Grid.SetColumn(searchGlyph, 0);

        Grid.SetColumn(_filterBox, 1);

        Grid.SetColumn(_clearFilterButton, 2);

        Grid.SetColumn(_sortButton, 3);
        Grid.SetColumn(_folderMenuButton, 4);

        row.Children.Add(searchGlyph);
        row.Children.Add(_filterBox);
        row.Children.Add(_clearFilterButton);
        row.Children.Add(_sortButton);
        row.Children.Add(_folderMenuButton);

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

    private MenuFlyout BuildSortFlyout()
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
        return new MenuFlyout { ItemsSource = new[] { az, za } };
    }

    // The "+" menu: add a folder, and — once at least one is watched — reveal or
    // stop watching one. There is no per-folder section header to hang these
    // off, now that a row's folder rides its tooltip rather than a grouping.
    private MenuFlyout BuildFolderFlyout()
    {
        var items = new List<Control>();
        var add = new MenuItem { Header = Loc.Chrome("import.folder.add") };
        add.Click += async (_, _) => await AddFolder();
        items.Add(add);

        if (_import.WatchedFolders.Count > 0)
        {
            items.Add(new Separator());
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

    // Rebuilds the tab bar, the progress line, and the tab content on every
    // ImportStore.Changed tick. The filter TextBox itself is never touched here
    // — it is the source of truth for what the user typed, not a mirror of it.
    private void Render()
    {
        var retainedSelection = CandidateSelectionModel.Retain(
            _selectedKey,
            TriageListModel.CandidateKeys(_import.TriageQueue));
        if (retainedSelection != _selectedKey)
        {
            _selectedKey = retainedSelection;
            _pane.Clear();
        }
        RenderTabBar();
        _clearFilterButton.IsVisible = _import.FilterText.Length > 0;
        RenderProgress();
        RenderContent();
    }

    private void RenderTabBar()
    {
        _tabBarHost.Children.Clear();
        var row = new Grid { ColumnDefinitions = new ColumnDefinitions("*,*,*,*"), ColumnSpacing = 4 };
        var counts = _import.TriageQueue.Counts;
        AddTabSegment(row, 0, BridgeTriageTab.NeedsYou, Loc.Chrome("import.tab.needs_you"), counts.NeedsYou);
        AddTabSegment(row, 1, BridgeTriageTab.Ready, Loc.Chrome("import.tab.ready"), counts.Ready);
        AddTabSegment(row, 2, BridgeTriageTab.Done, Loc.Chrome("import.tab.done"), counts.Done);
        AddTabSegment(row, 3, BridgeTriageTab.Skipped, Loc.Chrome("import.tab.skipped"), counts.Skipped);
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
        foreach (var scan in _import.TriageQueue.FolderScanStatuses)
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
            column.Children.Add(labelRow);
            column.Children.Add(ThinProgressBar(
                (double)progress.Identified / progress.Total));
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

    // ── Content: tab dispatch ────────────────────────────────────────────────

    private void RenderContent()
    {
        Control content = _import.ActiveTab switch
        {
            BridgeTriageTab.Ready => RenderReady(),
            BridgeTriageTab.NeedsYou => RenderNeedsYou(),
            BridgeTriageTab.Done => RenderDone(),
            BridgeTriageTab.Skipped => RenderSkipped(),
            _ => new Panel(),
        };
        _contentSlot.Content = content;
    }

    private Control EmptyState(bool filtering)
    {
        var text = new TextBlock
        {
            Text = filtering ? Loc.Chrome("import.empty.no_matches") : Loc.Chrome("import.empty.nothing_here"),
            FontSize = 13,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return new Border { Child = text, HorizontalAlignment = HorizontalAlignment.Stretch, VerticalAlignment = VerticalAlignment.Stretch };
    }

    private Control RenderReady()
    {
        var rows = TriageListModel.SelectableReadyRows(
            _import.TriageQueue,
            _import.FilterText,
            _import.SortOrder);
        if (rows.Count == 0)
        {
            return EmptyState(_import.FilterText.Length > 0);
        }

        var scroller = RenderReleaseSections(BridgeTriageTab.Ready);

        var readyKeys = rows.Select(row => row.CandidateKey).ToList();
        var selectedCount = _import.SelectedReady.Count(readyKeys.Contains);
        var footBar = BuildFootBar(selectedCount, readyKeys);

        var column = new Grid { RowDefinitions = new RowDefinitions("*,Auto") };
        Grid.SetRow(scroller, 0);
        Grid.SetRow(footBar, 1);
        column.Children.Add(scroller);
        column.Children.Add(footBar);
        return column;
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
        var keys = _import.SelectedReady.ToList();
        if (keys.Count == 0)
        {
            return;
        }

        var (settingsCurrent, settings) = _app.Settings.GetSettings();
        if (!settingsCurrent)
        {
            return;
        }
        var storageMode = settings.HasCloudHome ? "managed" : "unmanaged";
        var pinned = settings.HasCloudHome;

        var failureCount = 0;
        foreach (var key in keys)
        {
            // Selection can outlive the row that earned it (imported by a
            // faster sibling call, or reclassified) — the list content already
            // intersects the selection against the tab's current Ready keys,
            // so a miss here is defensive, not expected.
            var row = TriageListModel.Row(_import.TriageQueue, key);
            if (row?.Claim is not { } claim)
            {
                continue;
            }

            // A Ready row's decision is already stored — its settled single
            // match — so the commit reads the same answer opening the pane
            // would, from the archive.
            var (decidedCurrent, decidedResult) = await _app.Import.CandidateDecidedIdentity(key);
            if (!decidedCurrent)
            {
                return;
            }
            if (decidedResult.Decided is not { Release: not null } decided)
            {
                failureCount++;
                continue;
            }

            var (importCurrent, error) = await _app.Import.CommitImport(
                key, key, claim, storageMode, pinned, decided.Edit.Edit, null);
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

    private Control RenderNeedsYou()
    {
        var sections = TriageListModel.Sections(
            _import.TriageQueue,
            BridgeTriageTab.NeedsYou,
            _import.FilterText,
            _import.SortOrder);
        if (sections.Count == 0)
        {
            return EmptyState(_import.FilterText.Length > 0);
        }
        return RenderReleaseSections(BridgeTriageTab.NeedsYou);
    }

    private Control RenderDone()
    {
        var sections = TriageListModel.Sections(
            _import.TriageQueue,
            BridgeTriageTab.Done,
            _import.FilterText,
            _import.SortOrder);
        if (sections.Count == 0)
        {
            return EmptyState(_import.FilterText.Length > 0);
        }
        return RenderReleaseSections(BridgeTriageTab.Done);
    }

    private Control RenderSkipped()
    {
        var sections = TriageListModel.Sections(
            _import.TriageQueue,
            BridgeTriageTab.Skipped,
            _import.FilterText,
            _import.SortOrder);
        if (sections.Count == 0)
        {
            return EmptyState(_import.FilterText.Length > 0);
        }
        return RenderReleaseSections(BridgeTriageTab.Skipped);
    }

    private ScrollViewer RenderReleaseSections(BridgeTriageTab tab)
    {
        var list = new StackPanel { Spacing = 0 };
        foreach (var section in TriageListModel.Sections(
            _import.TriageQueue,
            tab,
            _import.FilterText,
            _import.SortOrder))
        {
            var content = new StackPanel { Spacing = 0 };
            foreach (var entry in section.Entries)
            {
                var control = entry.Bridge switch
                {
                    BridgeTriageEntry.Candidate candidate => BuildRow(candidate.Row),
                    BridgeTriageEntry.Boundary boundary => BuildBoundaryRow(boundary.BoundaryValue),
                    BridgeTriageEntry.Invalid invalid => BuildInvalidRow(invalid.InvalidCandidate),
                    _ => new Panel(),
                };
                control.Tag = entry.StableKey;
                content.Children.Add(control);
            }
            if (section.Group is { } group)
            {
                var expander = new Expander
                {
                    Header = group.Name,
                    Content = content,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    IsExpanded = _import.Interaction.IsGroupExpanded(
                        ImportStore.GroupDisclosureKey(group.Key)),
                    Padding = new Thickness(10, 4),
                };
                expander.PropertyChanged += (_, change) =>
                {
                    if (change.Property == Expander.IsExpandedProperty)
                    {
                        _import.Interaction.SetGroupExpanded(
                            ImportStore.GroupDisclosureKey(group.Key),
                            expander.IsExpanded);
                    }
                };
                var combine = new MenuItem
                {
                    Header = Loc.Chrome("import.release.one"),
                };
                combine.Click += (_, _) => ApplyFolderReleaseDecision(
                    group.Key,
                    BridgeFolderReleaseDecision.CombineAsOneRelease);
                var separate = new MenuItem
                {
                    Header = Loc.Chrome("import.release.separate"),
                };
                separate.Click += (_, _) => ApplyFolderReleaseDecision(
                    group.Key,
                    BridgeFolderReleaseDecision.KeepAsSeparateReleases);
                expander.ContextFlyout = new MenuFlyout
                {
                    ItemsSource = new[] { combine, separate },
                };
                list.Children.Add(expander);
            }
            else
            {
                list.Children.Add(content);
            }
        }
        return new ScrollViewer { Content = list };
    }

    private Control BuildBoundaryRow(BridgeFolderReleaseBoundary boundary)
    {
        var content = new StackPanel { Spacing = 5, Margin = new Thickness(14, 9) };
        var title = new TextBlock
        {
            Text = boundary.Name,
            FontSize = 14,
            FontWeight = FontWeight.SemiBold,
        };
        var tree = new StackPanel { Spacing = 3 };
        var displayPath = new TextBlock
        {
            Text = boundary.DisplayPath,
            FontFamily = new FontFamily("monospace"),
            FontSize = 11.5,
        };
        displayPath[!TextBlock.ForegroundProperty] =
            new DynamicResourceExtension("BaeTextSecondaryBrush");
        tree.Children.Add(displayPath);
        if (boundary.SharedFileCount > 0)
        {
            var sharedFiles = new TextBlock
            {
                Text = Loc.Chrome("storage.files", "count", (long)boundary.SharedFileCount),
                FontSize = 11.5,
            };
            sharedFiles[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            tree.Children.Add(sharedFiles);
        }
        foreach (var row in boundary.TreeRows)
        {
            var detail = row.Kind switch
            {
                BridgeFolderReleaseTreeRowKind.Candidate candidate =>
                    $"{candidate.TrackCount.ToString(CultureInfo.CurrentCulture)} · {candidate.FormatLabel}",
                BridgeFolderReleaseTreeRowKind.Invalid invalid =>
                    BridgeDisplay.LocalizedLine(invalid.Reason),
                _ => null,
            };
            var text = new TextBlock
            {
                Text = detail is null ? row.Name : $"{row.Name}  {detail}",
                FontSize = 11.5,
                Margin = new Thickness(row.Depth * 12, 0, 0, 0),
            };
            text[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension(
                    row.Kind is BridgeFolderReleaseTreeRowKind.Invalid
                        ? "BaeDangerBrush"
                        : "BaeTextSecondaryBrush");
            text.ContextMenu = BuildDecisionMenu(row.DecisionKey);
            tree.Children.Add(text);
        }
        content.Children.Insert(0, title);
        content.Children.Insert(1, tree);
        var one = new Button
        {
            Content = Loc.Chrome("import.release.one"),
            Padding = new Thickness(10, 4),
        };
        one.Click += (_, _) => ApplyFolderReleaseDecision(
            boundary.Key,
            BridgeFolderReleaseDecision.CombineAsOneRelease);
        var separate = new Button
        {
            Content = Loc.Chrome("import.release.separate"),
            Padding = new Thickness(10, 4),
        };
        separate.Click += (_, _) => ApplyFolderReleaseDecision(
            boundary.Key,
            BridgeFolderReleaseDecision.KeepAsSeparateReleases);
        var actions = new StackPanel
        {
            Orientation = Orientation.Vertical,
            Spacing = 6,
            Children = { one, separate },
        };
        one.HorizontalAlignment = HorizontalAlignment.Stretch;
        separate.HorizontalAlignment = HorizontalAlignment.Stretch;
        content.Children.Add(actions);
        return new Border { Child = content };
    }

    private ContextMenu BuildDecisionMenu(BridgeFolderReleaseDecisionKey key)
    {
        var combine = new MenuItem
        {
            Header = Loc.Chrome("import.release.one"),
        };
        combine.Click += (_, _) => ApplyFolderReleaseDecision(
            key,
            BridgeFolderReleaseDecision.CombineAsOneRelease);
        var separate = new MenuItem
        {
            Header = Loc.Chrome("import.release.separate"),
        };
        separate.Click += (_, _) => ApplyFolderReleaseDecision(
            key,
            BridgeFolderReleaseDecision.KeepAsSeparateReleases);
        return new ContextMenu { ItemsSource = new[] { combine, separate } };
    }

    // ── Rows ──────────────────────────────────────────────────────────────────

    // One triage row: a state stripe, an optional bulk-select checkbox, the
    // matched release's cover, its title/metadata, and trailing evidence or
    // status. Every decision about what the row shows — which tab, which
    // group, whether it takes a checkbox — is `row`'s, read off
    // BridgeTriageRow; this only renders it.
    private Control BuildRow(BridgeTriageRow row)
    {
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("3,26,44,*,Auto") };

        var stripe = new Border();
        if (StripeBrushKey(row) is { } stripeBrushKey)
        {
            stripe[!Border.BackgroundProperty] = new DynamicResourceExtension(stripeBrushKey);
        }
        Grid.SetColumn(stripe, 0);
        grid.Children.Add(stripe);

        var checkboxSlot = new Panel { Width = 26, Height = 18, VerticalAlignment = VerticalAlignment.Top, Margin = new Thickness(0, 7, 0, 0) };
        if (row.Selectable)
        {
            checkboxSlot.Children.Add(BuildCheckbox(row.CandidateKey));
        }
        Grid.SetColumn(checkboxSlot, 1);
        grid.Children.Add(checkboxSlot);

        var cover = BuildCover(row.Matched?.CoverThumbnailUrl);
        cover.Margin = new Thickness(0, 7, 10, 7);
        Grid.SetColumn(cover, 2);
        grid.Children.Add(cover);

        var text = BuildRowText(row);
        text.Margin = new Thickness(0, 7, 8, 7);
        Grid.SetColumn(text, 3);
        grid.Children.Add(text);

        var trailing = BuildRowTrailing(row);
        trailing.Margin = new Thickness(0, 9, 10, 7);
        trailing.VerticalAlignment = VerticalAlignment.Top;
        Grid.SetColumn(trailing, 4);
        grid.Children.Add(trailing);

        var isPending = row.Placement is BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.StillIdentifying };
        var host = new Border
        {
            Child = grid,
            Opacity = isPending || !row.Actionable ? 0.6 : 1,
            Background = Brushes.Transparent,
            IsEnabled = row.Actionable,
        };
        if (row.CandidateKey == _selectedKey)
        {
            host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeSelectionTintBrush");
        }
        ToolTip.SetTip(host, row.DisplayPath);
        host.Tapped += (_, _) => OnRowActivated(row);
        host.ContextMenu = BuildRowContextMenu(row);
        return host;
    }

    private static string? StripeBrushKey(BridgeTriageRow row) => row.Placement switch
    {
        BridgeTriagePlacement.NeedsYou { Group: BridgeNeedsYouGroup.SignalsDisagree } => "BaeDangerBrush",
        BridgeTriagePlacement.NeedsYou { Group: BridgeNeedsYouGroup.PickAPressing or BridgeNeedsYouGroup.CountsOrLengthsDisagree } => "BaeWarningBrush",
        BridgeTriagePlacement.NeedsYou { Group: BridgeNeedsYouGroup.AlreadyInLibrary } => "BaeInfoBrush",
        BridgeTriagePlacement.Done when row.ImportStatus is BridgeCandidateImportStatus.Error => "BaeDangerBrush",
        _ => null,
    };

    private Control BuildCheckbox(string key)
    {
        var isSelected = _import.SelectedReady.Contains(key);
        var box = new Border
        {
            Width = 18,
            Height = 18,
            CornerRadius = new CornerRadius(5),
            BorderThickness = new Thickness(isSelected ? 0 : 1.5),
        };
        if (isSelected)
        {
            box[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        }
        else
        {
            box.Background = Brushes.Transparent;
        }
        box[!Border.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        if (isSelected)
        {
            var check = new TextBlock { Text = "✓", FontSize = 11, FontWeight = FontWeight.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
            check[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeOnAccentBrush");
            box.Child = check;
        }
        var button = new Button
        {
            Content = box,
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        button.Click += (_, _) => _import.ToggleReadySelection(key);
        button.Tapped += (_, eventArgs) => eventArgs.Handled = true;
        return button;
    }

    private Control BuildCover(string? coverThumbnailUrl)
    {
        var image = new Image { Width = 44, Height = 44, Stretch = Stretch.UniformToFill };
        var host = new Border { Width = 44, Height = 44, CornerRadius = new CornerRadius(8), ClipToBounds = true, Child = image };
        host[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        if (!string.IsNullOrEmpty(coverThumbnailUrl))
        {
            _app.Images.Bind(image, new ImageContent.Remote(coverThumbnailUrl), ImageWidths.Row);
        }
        return host;
    }

    private Control BuildRowText(BridgeTriageRow row)
    {
        var column = new StackPanel { Spacing = 0 };
        var title = new TextBlock
        {
            Text = TriageListModel.DisplayTitle(row),
            FontSize = 14,
            FontWeight = FontWeight.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        title[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        column.Children.Add(title);

        if (RowSubLine(row) is { Length: > 0 } subLine)
        {
            var sub = new TextBlock
            {
                Text = subLine,
                FontSize = 12.5,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Margin = new Thickness(0, 1, 0, 0),
            };
            sub[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
            column.Children.Add(sub);
        }

        if (row.CandidateKey == _selectedKey)
        {
            var path = new TextBlock
            {
                Text = row.DisplayPath,
                FontFamily = new FontFamily("monospace"),
                FontSize = 11.5,
                MaxLines = 1,
                TextTrimming = TextTrimming.CharacterEllipsis,
                Margin = new Thickness(0, 4, 0, 0),
            };
            path[!TextBlock.ForegroundProperty] =
                new DynamicResourceExtension("BaeTextSecondaryBrush");
            column.Children.Add(path);
        }

        if (row.ImportStatus is BridgeCandidateImportStatus.Importing importing)
        {
            var bar = ThinProgressBar(importing.ProgressPercent / 100.0);
            bar.Margin = new Thickness(0, 7, 0, 0);
            column.Children.Add(bar);
        }

        var actions = BuildRowActions(row);
        if (actions is not null)
        {
            actions.Margin = new Thickness(0, 7, 0, 0);
            column.Children.Add(actions);
        }

        return column;
    }

    // The second line: the matched release's metadata, a disagreement sentence,
    // the still-identifying phase, or an import failure — whichever `row` is
    // actually saying.
    private static string? RowSubLine(BridgeTriageRow row) => row.Placement switch
    {
        BridgeTriagePlacement.Ready or BridgeTriagePlacement.Skipped => MetadataLine(row),
        BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.StillIdentifying phase } =>
            BridgeDisplay.LocalizedLine(phase.Phase),
        BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.AlreadyInLibrary, BridgeNeedsYouReason.Disagreement) =>
            MetadataLine(row),
        BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.PickAPressing, BridgeNeedsYouReason.Disagreement) =>
            row.Matched?.Artist,
        BridgeTriagePlacement.NeedsYou { Reason: BridgeNeedsYouReason.Disagreement disagreement } =>
            BridgeDisplay.LocalizedLine(disagreement.DisagreementValue),
        BridgeTriagePlacement.Done => DoneSubLine(row),
        _ => null,
    };

    private static string? MetadataLine(BridgeTriageRow row)
    {
        if (row.Matched is not { } matched)
        {
            return null;
        }
        var parts = new List<string>();
        if (!string.IsNullOrEmpty(matched.Artist))
        {
            parts.Add(matched.Artist!);
        }
        if (matched.Pressing is { } pressing)
        {
            if (pressing.Year is { } year)
            {
                parts.Add(year.ToString(CultureInfo.CurrentCulture));
            }
            if (!string.IsNullOrEmpty(pressing.Format))
            {
                parts.Add(pressing.Format!);
            }
            if (pressing.TrackCount is { } trackCount)
            {
                parts.Add(Loc.Chrome("import.candidate.tracks", "count", (long)trackCount));
            }
        }
        return parts.Count == 0 ? null : string.Join(" · ", parts);
    }

    private static string? DoneSubLine(BridgeTriageRow row) => row.ImportStatus switch
    {
        BridgeCandidateImportStatus.Importing importing => string.Join(
            " · ",
            new[]
            {
                importing.Step is { } step ? BridgeDisplay.LocalizedLine(step) : Loc.Chrome("import.progress.identifying"),
                (importing.ProgressPercent / 100.0).ToString("P0", CultureInfo.CurrentCulture),
            }.Where(part => part.Length > 0)),
        BridgeCandidateImportStatus.Complete or null => MetadataLine(row),
        BridgeCandidateImportStatus.Error error => BridgeDisplay.LocalizedLine(error.ErrorValue),
        _ => null,
    };

    // The pill row under the meta column — only the placements the design
    // gives one: pick-a-pressing, already-in-library, and a failed import.
    // Every other row's action is "activate it," which the whole row already
    // does.
    private Control? BuildRowActions(BridgeTriageRow row)
    {
        var pills = new List<(string Label, bool IsKey, Action Action)>();
        pills.AddRange(row.Placement switch
        {
            BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.PickAPressing, BridgeNeedsYouReason.Disagreement) =>
                new[]
                {
                    (Loc.Chrome("import.row.choose"), true, (Action)(() => OnRowActivated(row))),
                    (Loc.Chrome("import.candidate.skip"), false, (Action)(() => _import.SetCandidateSkipped(row.CandidateKey, true))),
                },
            BridgeTriagePlacement.NeedsYou(BridgeNeedsYouGroup.AlreadyInLibrary, BridgeNeedsYouReason.Disagreement) =>
                new[]
                {
                    (Loc.Chrome("import.candidate.skip"), false, (Action)(() => _import.SetCandidateSkipped(row.CandidateKey, true))),
                    (Loc.Chrome("import.row.import_anyway"), false, (Action)(() => OnRowActivated(row))),
                },
            BridgeTriagePlacement.Done when row.ImportStatus is BridgeCandidateImportStatus.Error =>
                new[]
                {
                    (Loc.Chrome("import.row.retry"), true, (Action)(() => OnRowActivated(row))),
                    (Loc.Chrome("libraries.reveal"), false, (Action)(() => RevealInFileManager.Reveal(row.CandidateKey))),
                },
            _ => Array.Empty<(string, bool, Action)>(),
        });
        foreach (var boundary in row.ResolvedBoundaries)
        {
            var decision = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
                : BridgeFolderReleaseDecision.CombineAsOneRelease;
            pills.Add((
                Loc.Chrome(
                    decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                        ? "import.release.one"
                        : "import.release.separate"),
                false,
                () => ApplyFolderReleaseDecision(boundary.Key, decision)));
        }
        if (pills.Count == 0)
        {
            return null;
        }
        var row2 = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        foreach (var (label, isKey, action) in pills)
        {
            row2.Children.Add(BuildActionPill(label, isKey, action));
        }
        return row2;
    }

    private static Button BuildActionPill(string label, bool isKey, Action onClick)
    {
        var button = new Button
        {
            Content = label,
            FontSize = 12,
            FontWeight = FontWeight.Medium,
            Padding = new Thickness(12, 4),
            CornerRadius = new CornerRadius(999),
            BorderThickness = new Thickness(isKey ? 0 : 1),
        };
        if (isKey)
        {
            button[!Button.BackgroundProperty] = new DynamicResourceExtension("BaeAccentBrush");
        }
        else
        {
            button.Background = Brushes.Transparent;
        }
        button[!Button.BorderBrushProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        button[!Button.ForegroundProperty] = new DynamicResourceExtension(isKey ? "BaeOnAccentBrush" : "BaeTextSecondaryBrush");
        button.Click += (_, _) => onClick();
        button.Tapped += (_, eventArgs) => eventArgs.Handled = true;
        return button;
    }

    private Control BuildRowTrailing(BridgeTriageRow row)
    {
        switch (row.Placement)
        {
            case BridgeTriagePlacement.Ready:
                return row.Matched is { } matched ? ReadyTrailing(matched.Evidence) : new Panel();
            case BridgeTriagePlacement.NeedsYou(var group, var reason):
                return NeedsYouTrailing(row, group, reason);
            case BridgeTriagePlacement.Done:
                return DoneTrailing(row);
            default:
                return new Panel();
        }
    }

    private static Control ReadyTrailing(BridgeMatchEvidence evidence)
    {
        var chip = new Border
        {
            CornerRadius = new CornerRadius(4),
            Padding = new Thickness(5, 2),
            Child = new TextBlock
            {
                Text = ProviderShortCode(evidence.Source),
                FontFamily = new FontFamily("monospace"),
                FontSize = 10,
                FontWeight = FontWeight.Medium,
            },
        };
        chip[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        ((TextBlock)chip.Child!)[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        var tip = evidence.Signal is { } signal
            ? $"{SignalLabel(signal)} · {ProviderDisplayName(evidence.Source)}"
            : ProviderDisplayName(evidence.Source);
        ToolTip.SetTip(chip, tip);
        return new StackPanel { HorizontalAlignment = HorizontalAlignment.Right, Children = { chip } };
    }

    private Control NeedsYouTrailing(BridgeTriageRow row, BridgeNeedsYouGroup group, BridgeNeedsYouReason reason)
    {
        switch (reason)
        {
            case BridgeNeedsYouReason.StillIdentifying stillIdentifying:
                return stillIdentifying.Phase == BridgeIdentifyPhase.Running
                    ? new Spinner { Width = 14, Height = 14 }
                    : DotIcon("BaeTextSecondaryBrush");
            case BridgeNeedsYouReason.Disagreement disagreement:
                return group switch
                {
                    BridgeNeedsYouGroup.PickAPressing => Chip(BridgeDisplay.LocalizedLine(disagreement.DisagreementValue), "BaeWarningBrush"),
                    BridgeNeedsYouGroup.SignalsDisagree => Chip(Loc.Chrome("import.row.conflict"), "BaeDangerBrush"),
                    BridgeNeedsYouGroup.AlreadyInLibrary => Chip(BridgeDisplay.LocalizedLine(disagreement.DisagreementValue), "BaeInfoBrush"),
                    BridgeNeedsYouGroup.CountsOrLengthsDisagree => DotIcon("BaeWarningBrush"),
                    BridgeNeedsYouGroup.NoMatch => SearchManuallyChip(row),
                    _ => new Panel(),
                };
            default:
                return new Panel();
        }
    }

    private Control SearchManuallyChip(BridgeTriageRow row)
    {
        var button = BuildActionPill(Loc.Chrome("import.row.search_manually"), isKey: false, () => OnRowActivated(row));
        button.Padding = new Thickness(7, 3);
        return button;
    }

    private Control DoneTrailing(BridgeTriageRow row) => row.ImportStatus switch
    {
        BridgeCandidateImportStatus.Importing => new Spinner { Width = 14, Height = 14 },
        BridgeCandidateImportStatus.Complete => DotIcon("BaeSuccessBrush", "✓"),
        BridgeCandidateImportStatus.Error => Chip(Loc.Chrome("import.row.failed"), "BaeDangerBrush"),
        // Already imported from a previous session (content-hash match), so
        // there is no in-session status to read — the fact is the same, so the
        // glyph is.
        null => DotIcon("BaeSuccessBrush", "✓"),
        _ => new Panel(),
    };

    private static Control DotIcon(string brushKey, string glyph = "•")
    {
        var text = new TextBlock { Text = glyph, FontSize = 12, FontWeight = FontWeight.Bold };
        text[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        return text;
    }

    // A tinted mono-text pill: the label at full strength over a wash of the
    // same brush at reduced opacity — two overlapping Borders rather than one,
    // since Avalonia's Opacity applies to the whole subtree (label included),
    // and the label needs to stay fully readable while the fill stays a wash.
    private static Control Chip(string? text, string brushKey)
    {
        var backdrop = new Border { CornerRadius = new CornerRadius(5), Opacity = 0.14 };
        backdrop[!Border.BackgroundProperty] = new DynamicResourceExtension(brushKey);

        var label = new TextBlock { Text = text, FontFamily = new FontFamily("monospace"), FontSize = 10.5 };
        label[!TextBlock.ForegroundProperty] = new DynamicResourceExtension(brushKey);
        var foreground = new Border { Padding = new Thickness(7, 3), Child = label };

        var host = new Panel();
        host.Children.Add(backdrop);
        host.Children.Add(foreground);
        return host;
    }

    private ContextMenu? BuildRowContextMenu(BridgeTriageRow row)
    {
        var items = new List<Control>();
        if (row.Placement is not BridgeTriagePlacement.Done)
        {
            var skipped = row.Placement is BridgeTriagePlacement.Skipped;
            var toggle = new MenuItem { Header = Loc.Chrome(skipped ? "import.candidate.unskip" : "import.candidate.skip") };
            toggle.Click += (_, _) => _import.SetCandidateSkipped(row.CandidateKey, !skipped);
            items.Add(toggle);
            items.Add(new Separator());
        }
        var reveal = new MenuItem { Header = Loc.Chrome("libraries.reveal") };
        reveal.Click += (_, _) => RevealInFileManager.Reveal(row.CandidateKey);
        items.Add(reveal);
        if (row.CombineAncestorKey is { } combineKey)
        {
            items.Add(new Separator());
            var combine = new MenuItem { Header = Loc.Chrome("import.release.one") };
            combine.Click += (_, _) => ApplyFolderReleaseDecision(
                combineKey,
                BridgeFolderReleaseDecision.CombineAsOneRelease);
            items.Add(combine);
        }
        foreach (var boundary in row.ResolvedBoundaries)
        {
            items.Add(new Separator());
            var decision = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
                : BridgeFolderReleaseDecision.CombineAsOneRelease;
            var regroup = new MenuItem
            {
                Header = Loc.Chrome(
                    decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                        ? "import.release.one"
                        : "import.release.separate"),
            };
            regroup.Click += (_, _) =>
                ApplyFolderReleaseDecision(boundary.Key, decision);
            items.Add(regroup);
        }
        return new ContextMenu { ItemsSource = items };
    }

    private Control BuildInvalidRow(BridgeInvalidCandidate invalid)
    {
        var name = new TextBlock
        {
            Text = invalid.SourceFolderName,
            FontSize = 14,
            FontWeight = FontWeight.SemiBold,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
        };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var reasonLine = BridgeDisplay.LocalizedLine(invalid.Reason);
        var reason = new TextBlock
        {
            Text = reasonLine,
            FontSize = 12.5,
            MaxLines = 1,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Margin = new Thickness(0, 1, 0, 0),
        };
        reason[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");

        var text = new StackPanel { Spacing = 0, Children = { name, reason } };
        var grid = new Grid { ColumnDefinitions = new ColumnDefinitions("3,26,44,*"), Margin = new Thickness(0, 7, 10, 7) };

        var stripe = new Border();
        stripe[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
        Grid.SetColumn(stripe, 0);
        var warning = new TextBlock { Text = "!", FontWeight = FontWeight.Bold, HorizontalAlignment = HorizontalAlignment.Center, VerticalAlignment = VerticalAlignment.Center };
        warning[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeDangerBrush");
        var iconBox = new Border { Width = 44, Height = 44, CornerRadius = new CornerRadius(8), Child = warning };
        iconBox[!Border.BackgroundProperty] = new DynamicResourceExtension("BaeElevatedBrush");
        Grid.SetColumn(iconBox, 2);
        Grid.SetColumn(text, 3);
        text.Margin = new Thickness(10, 0, 0, 0);
        text.VerticalAlignment = VerticalAlignment.Center;
        grid.Children.Add(stripe);
        grid.Children.Add(iconBox);
        grid.Children.Add(text);

        var reveal = new MenuItem { Header = Loc.Chrome("libraries.reveal") };
        reveal.Click += (_, _) => RevealInFileManager.Reveal(invalid.FolderPath);

        var host = new Border { Child = grid };
        ToolTip.SetTip(host, reasonLine);
        var items = new List<Control> { reveal };
        foreach (var boundary in invalid.ResolvedBoundaries)
        {
            items.Add(new Separator());
            var decision = boundary.Decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                ? BridgeFolderReleaseDecision.KeepAsSeparateReleases
                : BridgeFolderReleaseDecision.CombineAsOneRelease;
            var regroup = new MenuItem
            {
                Header = Loc.Chrome(
                    decision is BridgeFolderReleaseDecision.CombineAsOneRelease
                        ? "import.release.one"
                        : "import.release.separate"),
            };
            regroup.Click += (_, _) =>
                ApplyFolderReleaseDecision(boundary.Key, decision);
            items.Add(regroup);
        }
        host.ContextMenu = new ContextMenu { ItemsSource = items };
        return host;
    }

    private void ApplyFolderReleaseDecision(
        BridgeFolderReleaseDecisionKey key,
        BridgeFolderReleaseDecision decision)
    {
        _selectedKey = null;
        _pane.Clear();
        _import.SetFolderReleaseDecision(key, decision);
    }

    // ── Row-click activation ────────────────────────────────────────────────

    // Activate a row: kick off auto-identify for a candidate that hasn't been
    // looked at yet (there is nothing to map until it has been looked at),
    // otherwise put it under the mapping pane, read fresh for this key.
    private void OnRowActivated(BridgeTriageRow row)
    {
        if (!row.Actionable)
        {
            return;
        }
        if (row.Placement is BridgeTriagePlacement.NeedsYou
            {
                Reason: BridgeNeedsYouReason.StillIdentifying { Phase: BridgeIdentifyPhase.Queued },
            })
        {
            _ = _import.AutoIdentify(row.CandidateKey);
            return;
        }
        _selectedKey = row.CandidateKey;
        Render();
        _ = _pane.ShowCandidate(row);
    }

    // ── Evidence labels ──────────────────────────────────────────────────────

    // Brand names stay literal — never translated, matching every other
    // provider name in this app's chrome.
    private static string ProviderDisplayName(BridgeMetadataSource source) => source switch
    {
        BridgeMetadataSource.MusicBrainz => "MusicBrainz",
        BridgeMetadataSource.Discogs => "Discogs",
        _ => string.Empty,
    };

    // The row's compact trailing badge. Not a catalog key — an abbreviation of
    // a literal brand name is still the brand's name, not prose.
    private static string ProviderShortCode(BridgeMetadataSource source) => source switch
    {
        BridgeMetadataSource.MusicBrainz => "MB",
        BridgeMetadataSource.Discogs => "DC",
        _ => string.Empty,
    };

    private static string SignalLabel(BridgeMatchedSignal signal) => signal switch
    {
        BridgeMatchedSignal.DiscId => Loc.Chrome("import.row.disc_id"),
        BridgeMatchedSignal.Barcode => Loc.Chrome("import.row.barcode"),
        _ => string.Empty,
    };
}
