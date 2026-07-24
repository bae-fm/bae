using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The import section: a folder scan source plus the live candidate list bound
// to the import store, hosted in the main window's content area and swapped in
// by the Library/Import switcher (the macOS import section's counterpart).
// Clicking an unidentified candidate kicks off auto-identify; once it has a
// result, clicking opens the picker.
internal sealed class ImportSection
{
    private readonly SessionStore _session;
    private readonly Func<IntPtr> _windowHandle;
    private readonly ImportStore _import;
    private readonly ImportPickerDialog _picker;
    private readonly Action<string> _showImportError;
    // Switch the main window to the import section (the folder-drop and
    // activation flows land here with the section not necessarily active).
    private readonly Action _openSection;

    private bool _attached;

    public ImportSection(
        SessionStore session,
        Func<IntPtr> windowHandle,
        ImportStore import,
        ImportPickerDialog picker,
        Action<string> showImportError,
        Action openSection)
    {
        _session = session;
        _windowHandle = windowHandle;
        _import = import;
        _picker = picker;
        _showImportError = showImportError;
        _openSection = openSection;
    }

    // Scan a folder and land on its candidates — candidates stream into the
    // import store and the section (bound to that list) shows them, on a scan
    // error too, matching macOS, which navigates to import regardless of the
    // scan result. Shared by the window drop target and a folder activation
    // intent (the folder verb or bae://import); the caller has already
    // confirmed a library is open.
    public async System.Threading.Tasks.Task ImportFolder(string folderPath)
    {
        var (current, error) = await _import.ScanFolder(folderPath);
        if (!current)
        {
            return;
        }
        if (error is not null)
        {
            _showImportError(error);
        }

        _openSection();
    }

    // Entering the section (a pill switch or a folder flow): land on the New
    // tab with a fresh candidate list, as each macOS visit does.
    public void OnEntered()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }
        _import.SetActiveTab(CandidateTab.New);
        _import.RefreshCandidates();
    }

    // Build the section's content into its host once; the store subscriptions
    // live as long as the window, so entering and leaving the section only
    // toggles the host's visibility.
    public void Attach(Panel host)
    {
        if (_attached)
        {
            return;
        }
        _attached = true;

        var scanButton = new Button { Content = Loc.Chrome("import.choose_folder") };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        // The candidate refresh reports its no-releases line through the store;
        // render it into the status block without changing visibility (a scan
        // makes the block visible), matching the prior in-place update.
        void RenderScanStatus() => status.Text = _import.CandidatesStatusText;
        _import.CandidatesRefreshed += RenderScanStatus;

        // Watched-folders section: one row per watched folder, each with a remove
        // control. Rebuilt on every candidate refresh; collapses when empty.
        var foldersPanel = new StackPanel { Spacing = 4 };
        void RenderFolders()
        {
            foldersPanel.Children.Clear();
            foreach (var folder in _import.WatchedFolders)
            {
                foldersPanel.Children.Add(BuildFolderRow(folder));
            }
            foldersPanel.Visibility = _import.WatchedFolders.Count == 0
                ? Visibility.Collapsed
                : Visibility.Visible;
        }
        _import.CandidatesRefreshed += RenderFolders;

        // Tab bar: New / Added / Skipped, each labeled with its live count; the
        // active tab is emphasized. Clicking a tab shows it (absolute set).
        var newTab = new Button();
        var addedTab = new Button();
        var skippedTab = new Button();
        newTab.Click += (_, _) => _import.SetActiveTab(CandidateTab.New);
        addedTab.Click += (_, _) => _import.SetActiveTab(CandidateTab.Added);
        skippedTab.Click += (_, _) => _import.SetActiveTab(CandidateTab.Skipped);
        void RenderTabs()
        {
            var counts = _import.TabCounts;
            newTab.Content = Loc.Chrome("import.tab.new", "count", counts.New);
            addedTab.Content = Loc.Chrome("import.tab.added", "count", counts.Added);
            skippedTab.Content = Loc.Chrome("import.tab.skipped", "count", counts.Skipped);
            StyleTab(newTab, _import.ActiveTab == CandidateTab.New);
            StyleTab(addedTab, _import.ActiveTab == CandidateTab.Added);
            StyleTab(skippedTab, _import.ActiveTab == CandidateTab.Skipped);
        }
        _import.CandidatesRefreshed += RenderTabs;

        // Sort ComboBox: the four orders in enum order, so the selected index maps
        // straight to the value. Seed the selection before subscribing so the
        // initial value doesn't round-trip a save.
        var sortBox = new ComboBox { Header = Loc.Chrome("import.sort_by") };
        sortBox.Items.Add(Loc.Chrome("import.sort.name_az"));
        sortBox.Items.Add(Loc.Chrome("import.sort.name_za"));
        sortBox.Items.Add(Loc.Chrome("import.sort.newest"));
        sortBox.Items.Add(Loc.Chrome("import.sort.oldest"));
        sortBox.SelectedIndex = (int)_import.SortOrder;
        sortBox.SelectionChanged += (_, _) =>
        {
            if (sortBox.SelectedIndex >= 0)
            {
                _import.SetSortOrder((CandidateSortOrder)sortBox.SelectedIndex);
            }
        };

        var tabBar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        tabBar.Children.Add(newTab);
        tabBar.Children.Add(addedTab);
        tabBar.Children.Add(skippedTab);
        tabBar.Children.Add(sortBox);

        var list = new ListView
        {
            ItemsSource = _import.Candidates,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
        };

        // Each row builds imperatively: the candidate's one-line summary plus a
        // signals badge row. ContainerContentChanging fires per recycled container
        // (re-firing when UpdateCandidate swaps in a fresh instance), so the badges
        // refresh as identify events land.
        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue || args.Item is not ImportCandidate candidate)
            {
                return;
            }

            args.ItemContainer.Content = BuildCandidateRow(candidate);
            args.Handled = true;
        };

        // First click on an unidentified candidate kicks off auto-identification;
        // once it has a result, clicking opens the import dialog (auto matches
        // plus a manual-search fallback). The row reflects status from the
        // candidate snapshot.
        list.ItemClick += async (_, args) =>
        {
            if (args.ClickedItem is not ImportCandidate candidate)
            {
                return;
            }

            if (string.IsNullOrEmpty(candidate.Status))
            {
                _ = _import.AutoIdentify(candidate.Key, candidate.FolderPath);
            }
            else
            {
                await _picker.Show(candidate);
            }
        };

        // The picker needs the app window's handle in an unpackaged app. Adding
        // a watched folder starts scanning; invalidations refresh the candidates.
        scanButton.Click += async (_, _) =>
        {
            var picker = new global::Windows.Storage.Pickers.FolderPicker();
            picker.FileTypeFilter.Add("*");
            WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
            var folder = await picker.PickSingleFolderAsync();
            if (folder is null)
            {
                return;
            }

            status.Text = Loc.Chrome("import.scanning");
            status.Visibility = Visibility.Visible;
            var path = folder.Path;
            var (current, error) = await _import.ScanFolder(path);
            if (!current)
            {
                return;
            }
            if (error is not null)
            {
                status.Text = error;
            }
            else
            {
                _import.RefreshCandidates();
            }
        };

        // Controls stacked over the candidate list, which takes the remaining
        // height and scrolls itself (the ListView virtualizes).
        var controls = new StackPanel { Spacing = 8 };
        controls.Children.Add(scanButton);
        controls.Children.Add(foldersPanel);
        controls.Children.Add(status);
        controls.Children.Add(tabBar);

        var content = new Grid { RowSpacing = 8 };
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(controls, 0);
        Grid.SetRow(list, 1);
        content.Children.Add(controls);
        content.Children.Add(list);
        host.Children.Add(content);
    }

    // One candidate row: the summary line, then a signals badge row underneath
    // (omitted until the first toolbar event lands). Core pre-shapes every badge;
    // this iterates and renders.
    private StackPanel BuildCandidateRow(ImportCandidate candidate)
    {
        var row = new StackPanel { Spacing = 4 };
        row.Children.Add(new TextBlock
        {
            Text = candidate.ToString(),
            TextWrapping = TextWrapping.Wrap,
        });

        if (candidate.Signals.Count > 0)
        {
            row.Children.Add(SignalBadgeRow.Build(
                candidate.Signals,
                (kind, value) =>
                {
                    if (_session.CurrentHandleOrNull() != null)
                    {
                        _ = _import.ToggleSignal(candidate.Key, kind, value);
                    }
                },
                () =>
                {
                    if (_session.CurrentHandleOrNull() != null)
                    {
                        _ = _import.RerunIdentify(candidate.Key);
                    }
                }));
        }

        // Skip / un-skip rides a right-click flyout. An invalid folder can't be
        // skipped or unskipped (it always lists under Skipped), so it gets no
        // flyout — mirroring the macOS invalid-candidate row.
        if (!candidate.Invalid)
        {
            var flyout = new MenuFlyout();
            var toggleSkip = new MenuFlyoutItem
            {
                Text = candidate.Skipped
                    ? Loc.Chrome("import.candidate.unskip")
                    : Loc.Chrome("import.candidate.skip"),
            };
            toggleSkip.Click += (_, _) =>
                _import.SetCandidateSkipped(candidate.FolderPath, !candidate.Skipped);
            flyout.Items.Add(toggleSkip);
            row.ContextFlyout = flyout;
        }

        return row;
    }

    // One watched-folder row: the folder name (its path on hover) and a remove
    // control. Removing un-watches the folder straight away — no confirmation,
    // matching the macOS folder-header menu.
    private StackPanel BuildFolderRow(BridgeWatchedFolder folder)
    {
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Center,
        };

        var name = new TextBlock
        {
            Text = folder.Name,
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(name, folder.Path);
        row.Children.Add(name);

        var remove = new Button
        {
            Content = "✕",
            Padding = new Thickness(8, 3, 8, 3),
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(remove, Loc.Chrome("import.folder.remove"));
        remove.Click += (_, _) => _import.RemoveWatchedFolder(folder.Path);
        row.Children.Add(remove);

        return row;
    }

    // Emphasize the active tab and gray the inactive ones. The active tab reverts
    // to the default foreground so it reads normally in either theme.
    private static void StyleTab(Button tab, bool active)
    {
        tab.FontWeight = active
            ? Microsoft.UI.Text.FontWeights.SemiBold
            : Microsoft.UI.Text.FontWeights.Normal;
        if (active)
        {
            tab.ClearValue(Control.ForegroundProperty);
        }
        else
        {
            tab.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
        }
    }
}
