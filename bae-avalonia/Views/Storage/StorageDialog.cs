using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The storage sheet, presented in the window's modal host: the per-release storage
// table (select rows, right-click for the transitions they allow), the cloud
// outbox, the pin-download queue, the export queue, and the transfer-concurrency
// pickers. The table pages incrementally; the panels and rows refresh live while
// open through projection registrations disposed on close. Every read/write runs
// through a domain service — no view here touches NativeBae.
internal sealed partial class StorageDialog
{
    private const ulong StoragePageSize = 100;

    private readonly AppService _app;
    private readonly ModalHost _host;

    public StorageDialog(AppService app, ModalHost host)
    {
        _app = app;
        _host = host;
    }

    public async Task Show()
    {
        if (_app.Session.CurrentHandleOrNull() is null)
        {
            return;
        }

        // The releases whose rows are selected; a right-click acts on the selection
        // (or just the tapped row when it isn't part of it).
        var selected = new HashSet<string>();
        // Per-release outbox progress the Storage cell reads for its upload badge —
        // bounded to releases with active outbox activity, refreshed by
        // LoadStorageRows and read without a refetch by StorageCellText.
        var outboxProgress = new Dictionary<string, BridgeUploadProgress>();
        // The loaded storage rows by release id — the incremental list's side store,
        // the id → row resolution the table and the row menu read.
        var rowsById = new Dictionary<string, BridgeStorageRow>();

        // The filter tab resets to All each open (macOS parity); the sort persists.
        // Both are server-side (the row set is paged), so changing either rebuilds
        // the list wholesale from offset 0.
        var activeTab = StorageTab.All;
        var (sortField, sortDirection) = StorageSortStore.Load();

        var status = DialogUi.Danger();
        var footer = Secondary(string.Empty);

        // The storage cell's precedence mirrors macOS: an in-flight transfer verb
        // (from the overlay or the row's own transfer action) wins over the outbox
        // upload badge, which wins over the resting state.
        string StorageCellText(BridgeStorageRow row)
        {
            var releaseId = row.Release.Id;
            var token = _app.TransferProgressStore.TokenFor(releaseId)
                ?? (row.Release.TransferAction is { } action ? BridgeDisplay.TransferActionToken(action) : null);
            if (token is not null && BridgeDisplay.TransferVerbKey(token) is { } verbKey)
            {
                return Loc.Core(verbKey);
            }
            if (outboxProgress.TryGetValue(releaseId, out var progress))
            {
                return UploadBadgeLabel(progress);
            }
            return DialogPrimitives.RestingStorageLabel(
                row.Release.StorageState == BridgeReleaseStorageState.Remote, row.Release.Pinned);
        }

        void Ingest(IReadOnlyList<BridgeStorageRow> rows)
        {
            foreach (var row in rows)
            {
                rowsById[row.Release.Id] = row;
            }
        }

        void OnListError(Exception exception)
        {
            if (exception is OperationCanceledException)
            {
                return;
            }
            status.Text = (exception as PageLoadException)?.Line ?? Loc.Chrome("storage.load_failed");
            status.IsVisible = true;
        }

        // One list instance per current tab/sort — the query is baked into the page
        // source, so a tab or sort change rebuilds this rather than mutating it.
        PaginatedList<BridgeStorageRow, string> BuildList()
        {
            var tab = activeTab;
            var field = sortField;
            var direction = sortDirection;
            var source = new LibraryPageSource<BridgeStorageRow>(
                () => _app.Library.StorageCount(tab),
                (offset, limit) =>
                {
                    var (current, result) = _app.Library.StoragePage(tab, field, direction, offset, limit);
                    return (current, result.Page?.Rows, result.Error);
                });
            return new PaginatedList<BridgeStorageRow, string>(source, row => row.Release.Id, Ingest, OnListError);
        }

        var list = BuildList();

        var table = new StorageTableView(
            () => list,
            id => rowsById.GetValueOrDefault(id),
            selected,
            StorageCellText);

        // ── Sortable column header ────────────────────────────────────────────────
        var headerHost = new Panel();
        void RenderHeader()
        {
            var grid = StorageGrid.MakeRow();

            void AddSortHeader(int column, StorageSortField field, bool rightAligned = false)
            {
                var active = sortField == field;
                var arrow = sortDirection == SortDirection.Ascending ? "↑" : "↓";
                var label = Loc.Chrome(StorageListModel.ColumnLabelKey(field));
                var button = new Button
                {
                    Content = active ? $"{label} {arrow}" : label,
                    Background = Brushes.Transparent,
                    BorderThickness = new Thickness(0),
                    Padding = new Thickness(2, 0),
                    HorizontalAlignment = rightAligned ? HorizontalAlignment.Right : HorizontalAlignment.Left,
                };
                button[!TemplatedControl.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
                button.Click += (_, _) =>
                {
                    (sortField, sortDirection) = StorageListModel.Toggle(sortField, sortDirection, field);
                    StorageSortStore.Save(sortField, sortDirection);
                    _ = LoadStorageRows();
                };
                Grid.SetColumn(button, column);
                grid.Children.Add(button);
            }

            AddSortHeader(0, StorageSortField.AlbumTitle);
            AddSortHeader(1, StorageSortField.ArtistNames);
            AddSortHeader(2, StorageSortField.Format);
            var storageHeader = Secondary(Loc.Chrome(StorageListModel.StorageColumnLabelKey));
            storageHeader.Padding = new Thickness(2, 0);
            Grid.SetColumn(storageHeader, 3);
            grid.Children.Add(storageHeader);
            AddSortHeader(4, StorageSortField.FileCount, rightAligned: true);
            AddSortHeader(5, StorageSortField.TotalSize, rightAligned: true);

            headerHost.Children.Clear();
            headerHost.Children.Add(grid);
        }

        // ── Tab bar ───────────────────────────────────────────────────────────────
        var tabBar = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        var tabButtons = new List<(StorageTab Tab, Button Button)>();
        foreach (var tab in new[] { StorageTab.All, StorageTab.Unmanaged, StorageTab.Managed, StorageTab.Uploading })
        {
            var value = tab;
            var button = new Button { Content = Loc.Chrome(StorageListModel.TabLabelKey(tab)) };
            button.Click += (_, _) =>
            {
                if (activeTab == value)
                {
                    return;
                }
                activeTab = value;
                selected.Clear();
                _ = LoadStorageRows();
            };
            tabButtons.Add((value, button));
            tabBar.Children.Add(button);
        }

        void RenderTabs()
        {
            foreach (var (tab, button) in tabButtons)
            {
                StyleTab(button, tab == activeTab);
            }
        }

        // Rebuild the list for the current tab/sort, fetch its first page and the
        // outbox snapshot, and refresh the header/footer. A wholesale reload: there
        // is no whole-library cache to re-render from, so tab/sort/invalidation all
        // route here.
        async Task LoadStorageRows()
        {
            // The per-release outbox progress drives the Storage cell's upload badge;
            // load it before the rows realize so the badge is right on first render.
            // A failed read leaves the rows without the badge rather than failing the
            // whole list.
            outboxProgress.Clear();
            var (outboxCurrent, outbox) = await _app.Sync.OutboxSnapshot();
            if (!outboxCurrent)
            {
                return;
            }
            if (outbox.Snapshot is { } snapshot)
            {
                foreach (var entry in snapshot.PerRelease)
                {
                    outboxProgress[entry.Key] = entry.Value;
                }
            }

            rowsById.Clear();
            list = BuildList();
            table.Rebind();
            await list.LoadInitialAsync();
            if (list.InitialLoadError is not null)
            {
                status.Text = Loc.Chrome("storage.load_failed");
                status.IsVisible = true;
                return;
            }
            status.IsVisible = false;
            // Force the first page so its cells render immediately and its ids are
            // known for the selection reconcile (further pages load on scroll).
            await list.LoadRangeAsync(0, (int)StoragePageSize);
            // Drop selections for releases no longer present after the reload.
            selected.IntersectWith(list.AllLoadedIds);
            table.RefreshHighlights();

            RenderTabs();
            RenderHeader();
            var releasesText = Loc.Chrome("storage.footer.releases", "count", (long)list.TotalCount);
            // The core aggregate sums file sizes over every release matching the tab,
            // independent of how many pages have loaded. Shown only once fetched —
            // absence, not a zero/partial stand-in, while a stale session drops it.
            var (sizeCurrent, totalSize) = await _app.Library.StorageTotalSize(activeTab);
            footer.Text = sizeCurrent
                ? $"{releasesText} · {Loc.Chrome("storage.footer.total", "size", Loc.Bytes(totalSize))}"
                : releasesText;
        }

        // ── Pin-download queue ────────────────────────────────────────────────────
        var downloadsPanel = new StackPanel { Spacing = 4 };
        async Task LoadDownloads()
        {
            downloadsPanel.Children.Clear();
            var (current, result) = await _app.Downloads.DownloadSnapshot();
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                status.Text = result.Error;
                status.IsVisible = true;
                return;
            }
            if (result.Snapshot is not { } snapshot)
            {
                status.Text = Loc.Chrome("storage.read_failed");
                status.IsVisible = true;
                return;
            }
            if (snapshot.Downloads.Length == 0)
            {
                return;
            }

            string StateLabel(BridgeDownloadOp op) => op.State switch
            {
                BridgeDownloadState.Active => Loc.Chrome("download.state.downloading"),
                BridgeDownloadState.Failed => Loc.Chrome("download.state.failed"),
                _ => Loc.Chrome("download.state.queued"),
            };

            string DownloadDetail(BridgeDownloadOp op)
            {
                var parts = new List<string>
                {
                    Loc.Chrome("storage.files", "count", op.FileCount),
                    Loc.Bytes(op.TotalSize),
                    StateLabel(op),
                };
                if (DownloadProgress(op.State) is { } progress)
                {
                    parts.Add(Loc.Core(
                        "core.download.bytes_progress",
                        new Dictionary<string, object?>
                        {
                            ["done"] = Loc.Bytes(checked((long)progress.BytesDone)),
                            ["total"] = Loc.Bytes(checked((long)progress.BytesTotal)),
                        }));
                }
                return string.Join(" · ", parts);
            }

            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(Primary(snapshot.Paused ? Loc.Chrome("download.paused") : Loc.Chrome("download.title")));
            var retry = new Button { Content = Loc.Chrome("outbox.retry_now"), IsEnabled = snapshot.Total.Failed > 0 };
            retry.Click += async (_, _) =>
            {
                retry.IsEnabled = false;
                _app.Downloads.RetryDownloads();
                await LoadDownloads();
            };
            band.Children.Add(retry);
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                _app.Downloads.SetDownloadsPaused(!paused);
                await LoadDownloads();
            };
            band.Children.Add(pause);
            downloadsPanel.Children.Add(band);

            foreach (var op in snapshot.Downloads)
            {
                var itemGrid = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto"), ColumnSpacing = 8 };
                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(Primary(op.Title));
                labelColumn.Children.Add(Secondary(DownloadDetail(op)));
                if (DownloadProgress(op.State) is { } progress)
                {
                    labelColumn.Children.Add(new ProgressBar { Minimum = 0, Maximum = 1, Value = progress.Fraction, Height = 4 });
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                var releaseId = op.ReleaseId;
                var cancel = new Button { Content = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    status.IsVisible = false;
                    cancel.IsEnabled = false;
                    var (cancelCurrent, error) = await _app.Sync.CancelReleaseTransition(releaseId);
                    if (!cancelCurrent)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        status.Text = error;
                        status.IsVisible = true;
                        cancel.IsEnabled = true;
                        return;
                    }
                    await LoadDownloads();
                };
                Grid.SetColumn(cancel, 1);
                itemGrid.Children.Add(cancel);
                downloadsPanel.Children.Add(itemGrid);
            }
        }

        // ── Export (save/export) queue ────────────────────────────────────────────
        var exportsPanel = new StackPanel { Spacing = 4 };
        async Task LoadExports()
        {
            exportsPanel.Children.Clear();
            var (current, snapshot) = await _app.Downloads.OutputSnapshot();
            if (!current)
            {
                return;
            }
            if (snapshot.Outputs.Length == 0)
            {
                return;
            }

            static OutputKind OutputKindOf(BridgeOutputOp op) =>
                op.Kind is BridgeOutputKind.Save ? OutputKind.Save : OutputKind.Export;

            string StateLabel(BridgeOutputOp op) => op.State switch
            {
                BridgeOutputState.Active active => Loc.Chrome(
                    OutputQueueModel.StateKey(OutputRowKind.Active, OutputKindOf(op)),
                    "percent", OutputQueueModel.ClampPercent((int)active.Percent)),
                BridgeOutputState.Failed => Loc.Chrome(OutputQueueModel.StateKey(OutputRowKind.Failed, OutputKindOf(op))),
                _ => Loc.Chrome(OutputQueueModel.StateKey(OutputRowKind.Queued, OutputKindOf(op))),
            };

            static string PresetSuffix(BridgeOutputOp op) =>
                op.Kind is BridgeOutputKind.Save save ? $" · {save.PresetName}" : string.Empty;

            var paused = snapshot.Paused;
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(Primary(Loc.Chrome(OutputQueueModel.BandTitleKey(paused))));
            if (!paused && snapshot.SummaryParts.Length > 0)
            {
                band.Children.Add(Secondary(QueueSummaryText(snapshot.SummaryParts)));
            }
            var retry = new Button
            {
                Content = Loc.Chrome("outbox.retry_now"),
                IsEnabled = OutputQueueModel.RetryEnabled(snapshot.Total.Failed),
            };
            retry.Click += async (_, _) =>
            {
                retry.IsEnabled = false;
                _app.Downloads.RetryOutputs();
                await LoadExports();
            };
            band.Children.Add(retry);
            var pause = new Button { Content = Loc.Chrome(OutputQueueModel.PauseToggleKey(paused)) };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                _app.Downloads.SetOutputsPaused(!paused);
                await LoadExports();
            };
            band.Children.Add(pause);
            exportsPanel.Children.Add(band);

            foreach (var op in snapshot.Outputs)
            {
                var itemGrid = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto"), ColumnSpacing = 8 };
                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(Primary(op.Title));
                var detail = Secondary(
                    $"{Loc.Chrome("storage.files", "count", op.FileCount)} · {Loc.Bytes(op.TotalSize)}{PresetSuffix(op)} · {StateLabel(op)}");
                if (op.State is BridgeOutputState.Failed failed)
                {
                    ToolTip.SetTip(detail, failed.Error);
                }
                labelColumn.Children.Add(detail);
                if (op.State is BridgeOutputState.Active active)
                {
                    labelColumn.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = 100,
                        Value = OutputQueueModel.ClampPercent((int)active.Percent),
                        Height = 4,
                    });
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                var releaseId = op.ReleaseId;
                var cancel = new Button { Content = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    cancel.IsEnabled = false;
                    _app.Downloads.CancelOutput(releaseId);
                    await LoadExports();
                };
                Grid.SetColumn(cancel, 1);
                itemGrid.Children.Add(cancel);
                exportsPanel.Children.Add(itemGrid);
            }
        }

        // ── Cloud outbox (upload/delete queue) ────────────────────────────────────
        var outboxPanel = new StackPanel { Spacing = 4 };
        async Task LoadOutbox()
        {
            outboxPanel.Children.Clear();
            var (current, result) = await _app.Sync.OutboxSnapshot();
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                status.Text = result.Error;
                status.IsVisible = true;
                return;
            }
            if (result.Snapshot is not { } snapshot)
            {
                status.Text = Loc.Chrome("outbox.load_failed");
                status.IsVisible = true;
                return;
            }
            if (snapshot.UploadGroups.Length == 0 && snapshot.Deletes.Length == 0)
            {
                return;
            }

            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(Primary(OutboxSummary(snapshot)));
            var retry = new Button { Content = Loc.Chrome("outbox.retry_now") };
            retry.Click += async (_, _) =>
            {
                status.IsVisible = false;
                retry.IsEnabled = false;
                var (retryCurrent, error) = await _app.Sync.RetryOutbox();
                if (!retryCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.IsVisible = true;
                    retry.IsEnabled = true;
                    return;
                }
                await LoadOutbox();
            };
            band.Children.Add(retry);
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await _app.Sync.SetSyncPaused(!paused);
                await LoadOutbox();
            };
            band.Children.Add(pause);
            outboxPanel.Children.Add(band);

            // Master progress strip: a byte-progress bar (dimmed while paused) and
            // locale-formatted byte / throughput / ETA labels.
            if (snapshot.Total.BytesTotal > 0)
            {
                outboxPanel.Children.Add(new ProgressBar
                {
                    Minimum = 0,
                    Maximum = checked((long)snapshot.Total.BytesTotal),
                    Value = checked((long)snapshot.Total.BytesDone),
                    Opacity = paused ? 0.4 : 1.0,
                });
                var detail = new List<string>();
                foreach (var label in new[] { OutboxBytesLabel(snapshot), OutboxThroughputLabel(snapshot), OutboxEtaLabel(snapshot) })
                {
                    if (!string.IsNullOrEmpty(label))
                    {
                        detail.Add(label);
                    }
                }
                if (detail.Count > 0)
                {
                    outboxPanel.Children.Add(Secondary(string.Join(" · ", detail)));
                }
            }

            async Task RunCancel(Func<Task<(bool Current, string? Error)>> action)
            {
                status.IsVisible = false;
                var (cancelCurrent, error) = await action();
                if (!cancelCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    status.Text = error;
                    status.IsVisible = true;
                    return;
                }
                await LoadOutbox();
            }

            MenuFlyout CancelFlyout(Func<Task<(bool Current, string? Error)>> action)
            {
                var menu = new MenuFlyout();
                var item = new MenuItem { Header = Loc.Chrome("action.cancel") };
                item.Click += async (_, _) => await RunCancel(action);
                menu.Items.Add(item);
                return menu;
            }

            // Uploads: one expandable row per release — title, file count, cumulative
            // byte progress, an aggregate state badge, and the per-file list inside.
            // Right-click cancels the release's transition; the orphaned-files bucket
            // (no release id) has no release to cancel.
            foreach (var group in snapshot.UploadGroups)
            {
                var titleLine = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                titleLine.Children.Add(Primary(group.DisplayTitle));
                titleLine.Children.Add(Secondary(
                    $"{Loc.Chrome("storage.files", "count", group.Files.Length)} · {UploadBytesLabel(group.Progress)}"));
                titleLine.Children.Add(Secondary(UploadBadgeLabel(group.Progress)));
                var header = new StackPanel { Spacing = 2 };
                header.Children.Add(titleLine);
                if (group.Progress.BytesTotal > 0)
                {
                    header.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = checked((long)group.Progress.BytesTotal),
                        Value = checked((long)group.Progress.BytesDone),
                    });
                }

                var filesPanel = new StackPanel { Spacing = 2 };
                foreach (var file in group.Files)
                {
                    var fileGrid = new Grid { ColumnDefinitions = new ColumnDefinitions("*,Auto"), ColumnSpacing = 8 };
                    var fileColumn = new StackPanel { Spacing = 2 };
                    fileColumn.Children.Add(Primary($"{file.DisplayName} · {FileBytesLabel(file)}"));
                    if (file.State == BridgeUploadFileState.Uploading && file.BytesTotal > 0)
                    {
                        fileColumn.Children.Add(new ProgressBar
                        {
                            Minimum = 0,
                            Maximum = checked((long)file.BytesTotal),
                            Value = checked((long)file.BytesDone),
                        });
                    }
                    Grid.SetColumn(fileColumn, 0);
                    fileGrid.Children.Add(fileColumn);

                    var state = Secondary(FileStateLabel(file.State));
                    state.VerticalAlignment = VerticalAlignment.Center;
                    if (file.State == BridgeUploadFileState.Retrying && file.LastError is { } lastError)
                    {
                        ToolTip.SetTip(state, lastError);
                    }
                    Grid.SetColumn(state, 1);
                    fileGrid.Children.Add(state);
                    filesPanel.Children.Add(fileGrid);
                }

                var expander = new Expander
                {
                    Header = header,
                    Content = filesPanel,
                    IsExpanded = true,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                };
                if (group.ReleaseId is { } releaseId)
                {
                    expander.ContextFlyout = CancelFlyout(() => _app.Sync.CancelReleaseTransition(releaseId));
                }
                outboxPanel.Children.Add(expander);
            }

            // A pending delete carries no cancel: the object is already in the
            // cloud and the row that named it is gone, so abandoning the removal
            // would strand the object with nothing left to address it by.
            foreach (var delete in snapshot.Deletes)
            {
                var label = Primary(DeleteLabel(delete));
                label.VerticalAlignment = VerticalAlignment.Center;
                outboxPanel.Children.Add(label);
            }
        }

        // ── Device-local transfer concurrency (1..8) ──────────────────────────────
        // Always visible, unlike the queue panels that hide when idle; desktop
        // configures both directions. The picker is seeded before wiring the change
        // handler so seeding does not fire a spurious write.
        var transferPanel = new StackPanel { Spacing = 4 };
        Control MakeConcurrencyPicker(string header, uint current, Func<uint, (bool Current, string? Error)> apply)
        {
            var combo = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
            for (uint i = 1; i <= 8; i++)
            {
                combo.Items.Add(new ComboBoxItem { Content = i.ToString(), Tag = i });
            }
            combo.SelectedIndex = (int)current - 1;
            combo.SelectionChanged += (_, _) =>
            {
                if (combo.SelectedItem is ComboBoxItem { Tag: uint n })
                {
                    var (_, error) = apply(n);
                    if (error is not null)
                    {
                        status.Text = error;
                        status.IsVisible = true;
                    }
                }
            };
            var caption = Secondary(header, 12.5);
            return new StackPanel { Spacing = 4, Children = { caption, combo } };
        }

        void LoadTransferConfig()
        {
            transferPanel.Children.Clear();
            var (current, config) = _app.Settings.GetConfig();
            if (!current || config is not { } cfg)
            {
                return;
            }
            transferPanel.Children.Add(MakeConcurrencyPicker(
                Loc.Chrome("storage.concurrency.downloads"), cfg.MaxConcurrentDownloads, _app.Downloads.SetMaxConcurrentDownloads));
            transferPanel.Children.Add(MakeConcurrencyPicker(
                Loc.Chrome("storage.concurrency.uploads"), cfg.MaxConcurrentUploads, _app.Sync.SetMaxConcurrentUploads));
        }

        // ── The right-tap row menu ────────────────────────────────────────────────
        async Task ShowRowMenu(Control anchor)
        {
            var menu = await BuildStorageRowMenu(selected.ToList(), anchor);
            if (menu.Items.Count > 0)
            {
                menu.ShowAt(anchor, showAtPointer: true);
            }
        }

        async Task<MenuFlyout> BuildStorageRowMenu(List<string> releaseIds, Control anchor)
        {
            var menu = new MenuFlyout();

            // A release with a transition in flight offers only "Cancel" — the storage
            // actions would race it. Core dispatches the cancel to whichever queue is
            // running.
            var (transitioning, uploadError) = await _app.StorageStore.TransitioningReleases(releaseIds);
            if (uploadError is not null)
            {
                status.Text = uploadError;
                status.IsVisible = true;
            }
            if (transitioning.Count > 0)
            {
                var cancel = new MenuItem { Header = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    foreach (var releaseId in transitioning)
                    {
                        var (cancelCurrent, error) = await _app.Sync.CancelReleaseTransition(releaseId);
                        if (!cancelCurrent)
                        {
                            return;
                        }
                        if (error is not null)
                        {
                            status.Text = error;
                            status.IsVisible = true;
                            return;
                        }
                    }
                    await LoadStorageRows();
                };
                menu.Items.Add(cancel);
                return menu;
            }

            foreach (var action in _app.StorageStore.IntersectedStorageActions(releaseIds, rowsById))
            {
                var act = action;
                var item = new MenuItem { Header = DialogPrimitives.StorageActionLabel(act) };
                item.Click += async (_, _) =>
                {
                    var error = await _app.StorageStore.RunStorageActionForReleases(
                        act, releaseIds, () => DialogPrimitives.PickUnmanageFolder(anchor));
                    if (error is not null)
                    {
                        status.Text = error;
                        status.IsVisible = true;
                    }
                    else
                    {
                        await LoadStorageRows();
                    }
                };
                menu.Items.Add(item);
            }

            return menu;
        }

        // The row menu targets the presenter's selection and reloads the rows, so
        // it's wired after the table (and the state it closes over) exist.
        table.MenuCallback = ShowRowMenu;

        // ── Assemble, present, and refresh live ───────────────────────────────────
        var registrations = new List<IDisposable>();
        void OnTransfersChanged() => table.RefreshCells();
        _app.TransferProgressStore.Changed += OnTransfersChanged;
        try
        {
            await _host.Show(close =>
            {
                var content = new StackPanel { Spacing = 8, MinWidth = 640 };
                content.Children.Add(status);
                content.Children.Add(transferPanel);
                content.Children.Add(downloadsPanel);
                content.Children.Add(exportsPanel);
                content.Children.Add(outboxPanel);
                content.Children.Add(tabBar);
                content.Children.Add(headerHost);
                content.Children.Add(table);
                content.Children.Add(footer);

                var column = DialogUi.Column();
                column.MinWidth = 640;
                column.Children.Add(DialogUi.Title(Loc.Chrome("storage.title")));
                column.Children.Add(new ScrollViewer { Content = content, MaxHeight = 480 });
                var closeButton = new Button { Content = Loc.Chrome("action.close") };
                closeButton.Click += (_, _) => close();
                column.Children.Add(DialogUi.Actions(closeButton));

                // Refresh the panels live while open as uploads/deletes/pins/exports
                // progress; an album/release invalidation that isn't an outbox change
                // can still alter a release's storage state, so it refreshes the rows.
                registrations.Add(_app.ProjectionRegistry.Register(typeof(BridgeInvalidation.Outbox), () =>
                {
                    _ = LoadOutbox();
                    _ = LoadStorageRows();
                }));
                registrations.Add(_app.ProjectionRegistry.Register(typeof(BridgeInvalidation.DownloadQueue), () =>
                {
                    _ = LoadDownloads();
                    _ = LoadStorageRows();
                }));
                registrations.Add(_app.ProjectionRegistry.Register(typeof(BridgeInvalidation.OutputQueue), () => _ = LoadExports()));
                registrations.Add(_app.ProjectionRegistry.Register(typeof(BridgeInvalidation.Album), () => _ = LoadStorageRows()));
                registrations.Add(_app.ProjectionRegistry.Register(typeof(BridgeInvalidation.Release), () => _ = LoadStorageRows()));

                LoadTransferConfig();
                _ = LoadStorageRows();
                _ = LoadDownloads();
                _ = LoadExports();
                _ = LoadOutbox();

                return column;
            });
        }
        finally
        {
            _app.TransferProgressStore.Changed -= OnTransfersChanged;
            foreach (var registration in registrations)
            {
                registration.Dispose();
            }
        }
    }

    // Emphasize the active tab and gray the inactive ones, matching the import
    // dialog's tab bar.
    private static void StyleTab(Button tab, bool active)
    {
        tab.FontWeight = active ? FontWeight.SemiBold : FontWeight.Normal;
        if (active)
        {
            tab[!TemplatedControl.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        }
        else
        {
            tab[!TemplatedControl.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        }
    }

    private static TextBlock Primary(string text)
    {
        var block = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap };
        block[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextPrimaryBrush");
        return block;
    }

    private static TextBlock Secondary(string text, double size = 12)
    {
        var block = new TextBlock { Text = text, FontSize = size, TextWrapping = TextWrapping.Wrap };
        block[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");
        return block;
    }
}
