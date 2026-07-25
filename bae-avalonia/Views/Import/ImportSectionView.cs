using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Templates;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using Avalonia.Platform.Storage;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The import section: a folder scan source plus the live candidate list bound to
// the import store, shown in the shell's content area when the Library/Import
// switcher selects Import (the macOS import section's counterpart). Clicking an
// unidentified candidate kicks off auto-identify; once it has a result, clicking
// opens the identify/confirm dialog. Built once and kept for the window's lifetime
// (the store subscriptions live that long); entering the section refreshes it.
internal sealed class ImportSectionView : UserControl
{
    private readonly AppService _app;
    private readonly ImportStore _import;
    private readonly ImportDialogs _dialogs;

    private readonly TextBlock _status;
    private readonly StackPanel _foldersPanel;
    private readonly Button _newTab;
    private readonly Button _addedTab;
    private readonly Button _skippedTab;

    public ImportSectionView(AppService app, ImportDialogs dialogs)
    {
        _app = app;
        _import = app.ImportStore;
        _dialogs = dialogs;

        var scan = new Button { Content = Loc.Chrome("import.choose_folder") };
        scan.Click += OnScanClicked;

        _status = DialogUi.Body(string.Empty);
        _status.IsVisible = false;

        _foldersPanel = new StackPanel { Spacing = 4 };

        _newTab = new Button();
        _addedTab = new Button();
        _skippedTab = new Button();
        _newTab.Click += (_, _) => _import.SetActiveTab(CandidateTab.New);
        _addedTab.Click += (_, _) => _import.SetActiveTab(CandidateTab.Added);
        _skippedTab.Click += (_, _) => _import.SetActiveTab(CandidateTab.Skipped);

        // The four sort orders in enum order, so the selected index maps straight to
        // the value. Seed the selection before subscribing so it doesn't round-trip.
        var sortBox = new ComboBox
        {
            ItemsSource = new[]
            {
                Loc.Chrome("import.sort.name_az"),
                Loc.Chrome("import.sort.name_za"),
                Loc.Chrome("import.sort.newest"),
                Loc.Chrome("import.sort.oldest"),
            },
            SelectedIndex = (int)_import.SortOrder,
            VerticalAlignment = VerticalAlignment.Center,
        };
        sortBox.SelectionChanged += (_, _) =>
        {
            if (sortBox.SelectedIndex >= 0)
            {
                _import.SetSortOrder((CandidateSortOrder)sortBox.SelectedIndex);
            }
        };

        var tabBar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, VerticalAlignment = VerticalAlignment.Center };
        tabBar.Children.Add(_newTab);
        tabBar.Children.Add(_addedTab);
        tabBar.Children.Add(_skippedTab);
        tabBar.Children.Add(sortBox);

        var list = new ItemsControl
        {
            ItemsSource = _import.Candidates,
            ItemTemplate = new FuncDataTemplate<ImportCandidate>((candidate, _) => BuildCandidateRow(candidate), supportsRecycling: false),
        };

        var controls = new StackPanel { Spacing = 8 };
        controls.Children.Add(scan);
        controls.Children.Add(_foldersPanel);
        controls.Children.Add(_status);
        controls.Children.Add(tabBar);

        var content = new Grid { RowDefinitions = new RowDefinitions("Auto,*"), RowSpacing = 8, Margin = new Thickness(22, 24) };
        Grid.SetRow(controls, 0);
        var scroller = new ScrollViewer { Content = list };
        Grid.SetRow(scroller, 1);
        content.Children.Add(controls);
        content.Children.Add(scroller);
        Content = content;

        _import.CandidatesRefreshed += RenderStatus;
        _import.CandidatesRefreshed += RenderFolders;
        _import.CandidatesRefreshed += RenderTabs;
        RenderStatus();
        RenderFolders();
        RenderTabs();
    }

    // Entering the section (a switcher click or a folder-drop flow): land on the New
    // tab with a fresh candidate list, as each macOS visit does.
    public void OnEntered()
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }
        _import.SetActiveTab(CandidateTab.New);
        _import.RefreshCandidates();
    }

    private async void OnScanClicked(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        var storage = TopLevel.GetTopLevel(this)?.StorageProvider;
        if (storage is null)
        {
            return;
        }
        var folders = await storage.OpenFolderPickerAsync(new Avalonia.Platform.Storage.FolderPickerOpenOptions { AllowMultiple = false });
        if (folders.Count == 0 || folders[0].TryGetLocalPath() is not { } path)
        {
            return;
        }

        _status.Text = Loc.Chrome("import.scanning");
        _status.IsVisible = true;
        var (current, error) = await _import.ScanFolder(path);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _status.Text = error;
        }
        else
        {
            _import.RefreshCandidates();
        }
    }

    // The candidate refresh reports its no-releases line through the store; render it
    // without changing visibility (a scan makes the block visible).
    private void RenderStatus() => _status.Text = _import.CandidatesStatusText;

    // One row per watched folder, each with a remove control. Collapses when empty.
    private void RenderFolders()
    {
        _foldersPanel.Children.Clear();
        foreach (var folder in _import.WatchedFolders)
        {
            _foldersPanel.Children.Add(BuildFolderRow(folder));
        }
        _foldersPanel.IsVisible = _import.WatchedFolders.Count > 0;
    }

    // New / Added / Skipped, each labeled with its live count; the active tab is
    // emphasized.
    private void RenderTabs()
    {
        var counts = _import.TabCounts;
        _newTab.Content = Loc.Chrome("import.tab.new", "count", counts.New);
        _addedTab.Content = Loc.Chrome("import.tab.added", "count", counts.Added);
        _skippedTab.Content = Loc.Chrome("import.tab.skipped", "count", counts.Skipped);
        StyleTab(_newTab, _import.ActiveTab == CandidateTab.New);
        StyleTab(_addedTab, _import.ActiveTab == CandidateTab.Added);
        StyleTab(_skippedTab, _import.ActiveTab == CandidateTab.Skipped);
    }

    // One candidate row: the summary line, then a signals badge row underneath
    // (omitted until the first identify event lands). First click on an unidentified
    // candidate kicks off auto-identification; once it has a result, clicking opens
    // the identify/confirm dialog.
    private Control BuildCandidateRow(ImportCandidate candidate)
    {
        var summary = new TextBlock { Text = candidate.ToString(), TextWrapping = TextWrapping.Wrap };
        summary[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        var row = new StackPanel { Spacing = 4, Margin = new Thickness(0, 6) };
        row.Children.Add(summary);

        if (candidate.Signals.Count > 0)
        {
            row.Children.Add(SignalBadgeRow.Build(
                candidate.Signals,
                (kind, value) =>
                {
                    if (_app.Session.CurrentHandleOrNull() is not null)
                    {
                        _ = _import.ToggleSignal(candidate.Key, kind, value);
                    }
                },
                () =>
                {
                    if (_app.Session.CurrentHandleOrNull() is not null)
                    {
                        _ = _import.RerunIdentify(candidate.Key);
                    }
                }));
        }

        // Skip / un-skip rides a right-click flyout. An invalid folder can't be
        // skipped (it always lists under Skipped), so it gets no flyout.
        if (!candidate.Invalid)
        {
            var toggle = new MenuItem
            {
                Header = candidate.Skipped ? Loc.Chrome("import.candidate.unskip") : Loc.Chrome("import.candidate.skip"),
            };
            toggle.Click += (_, _) => _import.SetCandidateSkipped(candidate.FolderPath, !candidate.Skipped);
            row.ContextMenu = new ContextMenu { ItemsSource = new[] { toggle } };
        }

        var button = new Button
        {
            Background = Brushes.Transparent,
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            HorizontalContentAlignment = HorizontalAlignment.Left,
            Content = row,
        };
        button.Click += async (_, _) =>
        {
            if (string.IsNullOrEmpty(candidate.Status))
            {
                _ = _import.AutoIdentify(candidate.Key, candidate.FolderPath);
            }
            else
            {
                await _dialogs.ShowPicker(candidate);
            }
        };
        return button;
    }

    // One watched-folder row: the folder name (its path on hover) and a remove
    // control. Removing un-watches the folder straight away — no confirmation.
    private Control BuildFolderRow(BridgeWatchedFolder folder)
    {
        var name = new TextBlock { Text = folder.Name, VerticalAlignment = VerticalAlignment.Center };
        name[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        ToolTip.SetTip(name, folder.Path);

        var remove = new Button { Content = "✕", Padding = new Thickness(8, 3), VerticalAlignment = VerticalAlignment.Center };
        ToolTip.SetTip(remove, Loc.Chrome("import.folder.remove"));
        remove.Click += (_, _) => _import.RemoveWatchedFolder(folder.Path);

        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8, VerticalAlignment = VerticalAlignment.Center };
        row.Children.Add(name);
        row.Children.Add(remove);
        return row;
    }

    // Emphasize the active tab and de-emphasize the inactive ones.
    private static void StyleTab(Button tab, bool active)
    {
        tab.FontWeight = active ? FontWeight.SemiBold : FontWeight.Normal;
        tab[!Button.ForegroundProperty] = new DynamicResourceExtension(active ? "BaeTextPrimaryBrush" : "BaeTextSecondaryBrush");
    }
}
