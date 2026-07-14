using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The storage sheet: the per-release storage table (select rows, right-click for
// the transitions they allow), the cloud outbox (upload/delete queue), and the
// pin download queue. Rows and panels refresh live while open through projection
// registrations disposed on close. The non-UI operations (transition detection,
// running a transition, the action intersection) live on the storage store.
internal sealed class StorageDialog
{
    private const string AscendingArrow = "↑";
    private const string DescendingArrow = "↓";
    private const ulong StoragePageSize = 100;

    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly StorageStore _storage;
    private readonly ProjectionRegistry _projections;
    private readonly TransferProgressStore _transfers;

    public StorageDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        StorageStore storage,
        ProjectionRegistry projections,
        TransferProgressStore transfers)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _storage = storage;
        _projections = projections;
        _transfers = transfers;
    }

    public async System.Threading.Tasks.Task Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        var storageStatus = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // The releases whose rows are selected. Right-clicking applies the
        // chosen action to the whole selection (or to just the right-tapped row
        // when it isn't part of it). Releases that vanish on reload (e.g. an
        // local release moved out of the library) drop out below.
        var selected = new HashSet<string>();
        // The per-release outbox progress the Storage cell reads for its upload
        // badge — bounded to releases with active outbox activity (never the
        // whole library), refreshed by LoadStorageRows and read without a
        // refetch by StorageCellText.
        var outboxProgress = new Dictionary<string, BridgeUploadProgress>();
        // The Storage-column cell for each currently-realized row, keyed by
        // release id, so a transfer/outbox change can refresh just that cell —
        // bounded to loaded rows, evicted as containers recycle.
        var storageCellsByReleaseId = new Dictionary<string, TextBlock>();

        // Filter tab resets to All each open (macOS parity); the sort persists.
        // Both are now server-side (the row set is paged, so a client-side
        // filter/sort over only the loaded rows would be wrong), so changing
        // either resets and reloads from offset 0.
        var activeTab = StorageTab.All;
        var (sortField, sortDirection) = StorageSortStore.Load();

        var headerRow = MakeStorageGrid();
        var footer = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            FontSize = 12,
        };

        // The storage cell's precedence mirrors macOS: an in-flight transfer verb
        // (from the overlay or the row's own transfer action) wins over the outbox
        // upload badge, which wins over the resting state.
        string StorageCellText(BridgeStorageRow row)
        {
            var releaseId = row.Release.Id;
            var token = _transfers.TokenFor(releaseId)
                ?? (row.Release.TransferAction is { } action ? NativeBae.TransferActionToken(action) : null);
            if (token is not null && NativeBae.TransferActionKey(token) is { } verbKey)
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

        // Fetch one page of storage rows for the active tab/sort. offset 0 is
        // always the eager first page LoadStorageRows fetches directly; further
        // pages come through here via the collection's incremental loading.
        (bool Current, IReadOnlyList<BridgeStorageRow>? Items, string? Error) FetchStoragePage(ulong offset, ulong limit)
        {
            var (current, result) = _session.WithCurrentHandle(
                handle => NativeBae.StoragePage(handle, activeTab, sortField, sortDirection, offset, limit));
            if (!current)
            {
                return (false, null, null);
            }
            var (page, error) = result;
            if (error is not null || page is null)
            {
                return (true, null, error ?? Loc.Chrome("storage.load_failed"));
            }
            return (true, page.Rows, null);
        }

        var rows = new IncrementalBrowserCollection<BridgeStorageRow>(FetchStoragePage, error =>
        {
            storageStatus.Text = error ?? Loc.Chrome("storage.load_failed");
            storageStatus.Visibility = Visibility.Visible;
        });

        var list = new ListView
        {
            ItemsSource = rows,
            SelectionMode = ListViewSelectionMode.None,
            // Bounded so ItemsStackPanel actually virtualizes — this dialog's
            // content sits in an outer ScrollViewer (below), which would
            // otherwise hand the list unbounded height. Matches ImportDialog's
            // list.
            MaxHeight = 320,
            MinWidth = 560,
        };

        // The tab bar: All / Unmanaged / Managed / Uploading. Clicking a tab
        // clears the selection and reloads from the server with the new filter.
        // Built after `rows`/`list` exist: the click handler calls
        // LoadStorageRows, which reads both, so definite assignment requires
        // them declared first.
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

        // Recolor every currently-realized row from the current selection —
        // bounded to loaded/realized rows, never the whole library.
        void RefreshRowHighlights()
        {
            for (var i = 0; i < rows.Count; i++)
            {
                if (list.ContainerFromIndex(i) is ListViewItem { Content: Border border } && border.Tag is string id)
                {
                    border.Background = RowBackground(selected.Contains(id));
                }
            }
        }

        // Refresh the Storage cell of every currently-tracked (realized) row
        // from its latest transfer/outbox state — bounded to loaded rows.
        void RefreshStorageCells()
        {
            if (storageCellsByReleaseId.Count == 0)
            {
                return;
            }
            var loadedById = rows.ToDictionary(row => row.Release.Id);
            foreach (var (releaseId, cell) in storageCellsByReleaseId)
            {
                if (loadedById.TryGetValue(releaseId, out var row))
                {
                    cell.Text = StorageCellText(row);
                }
            }
        }

        // Build one row's visual: the six-column grid plus tap-to-select and
        // right-tap-for-menu. Returns the Storage cell too, so the caller can
        // track it for live refreshes.
        (Border RowVisual, TextBlock StorageCell) BuildStorageRowVisual(BridgeStorageRow row)
        {
            var releaseId = row.Release.Id;
            var grid = MakeStorageGrid();
            AddCell(grid, 0, row.Album.Title);
            AddCell(grid, 1, row.Album.ArtistNames);
            AddCell(grid, 2, row.Release.Format ?? string.Empty);
            var storageCell = AddCell(grid, 3, StorageCellText(row));
            AddCell(grid, 4, Loc.Number(row.Release.FileCount), rightAligned: true);
            AddCell(grid, 5, Loc.Bytes(row.Release.TotalSize), rightAligned: true);

            var rowBorder = new Border
            {
                Child = grid,
                // The release id rides on Tag so RefreshRowHighlights can
                // recolor each row from the current selection.
                Tag = releaseId,
                Padding = new Thickness(6, 4, 6, 4),
                CornerRadius = new CornerRadius(4),
                Background = RowBackground(selected.Contains(releaseId)),
            };

            rowBorder.Tapped += (_, _) =>
            {
                if (!selected.Add(releaseId))
                {
                    selected.Remove(releaseId);
                }
                rowBorder.Background = RowBackground(selected.Contains(releaseId));
            };
            rowBorder.RightTapped += async (_, args) =>
            {
                // The args are only valid synchronously; capture the tap
                // position before any await.
                var position = args.GetPosition(rowBorder);

                // Act on the selection when this row is part of it, else on
                // just this row (and select it, matching the macOS menu).
                if (!selected.Contains(releaseId))
                {
                    selected.Clear();
                    selected.Add(releaseId);
                    RefreshRowHighlights();
                }

                // Every release the user could have selected is still held by
                // `rows` (virtualization recycles containers, never the
                // underlying data), so this resolves the whole selection.
                var rowsById = rows.ToDictionary(r => r.Release.Id);
                var menu = await BuildStorageRowMenu(
                    selected.ToList(), rowsById, storageStatus, LoadStorageRows);
                // Nothing to offer (e.g. no cloud home, or uploads in flight)
                // — skip the empty popup.
                if (menu.Items.Count > 0)
                {
                    menu.ShowAt(rowBorder, new FlyoutShowOptions { Position = position });
                }
            };

            return (rowBorder, storageCell);
        }

        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue)
            {
                // Evict this container's previous row's cell reference before
                // it's reused for a different item.
                if (args.ItemContainer.Tag is string previousReleaseId)
                {
                    storageCellsByReleaseId.Remove(previousReleaseId);
                }
                return;
            }
            if (args.Item is not BridgeStorageRow row)
            {
                return;
            }
            var (rowVisual, storageCell) = BuildStorageRowVisual(row);
            args.ItemContainer.Content = rowVisual;
            args.ItemContainer.Tag = row.Release.Id;
            storageCellsByReleaseId[row.Release.Id] = storageCell;
            args.Handled = true;
        };

        // The sortable-column header: a button per column. The five data columns
        // toggle the sort (persisting it, then reloading from the server with the
        // new order) and show the ↑/↓ arrow when active; the Storage column is
        // inert.
        void RenderHeader()
        {
            headerRow.Children.Clear();

            void AddSortHeader(int column, StorageSortField field, bool rightAligned = false)
            {
                var active = sortField == field;
                var arrow = sortDirection == SortDirection.Ascending ? AscendingArrow : DescendingArrow;
                var label = Loc.Chrome(StorageListModel.ColumnLabelKey(field));
                var button = new Button
                {
                    Content = active ? $"{label} {arrow}" : label,
                    Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
                    BorderThickness = new Thickness(0),
                    Padding = new Thickness(2, 0, 2, 0),
                    HorizontalAlignment = rightAligned ? HorizontalAlignment.Right : HorizontalAlignment.Left,
                };
                button.Click += (_, _) =>
                {
                    (sortField, sortDirection) = StorageListModel.Toggle(sortField, sortDirection, field);
                    StorageSortStore.Save(sortField, sortDirection);
                    _ = LoadStorageRows();
                };
                Grid.SetColumn(button, column);
                headerRow.Children.Add(button);
            }

            AddSortHeader(0, StorageSortField.AlbumTitle);
            AddSortHeader(1, StorageSortField.ArtistNames);
            AddSortHeader(2, StorageSortField.Format);
            var storageHeader = new TextBlock
            {
                Text = Loc.Chrome(StorageListModel.StorageColumnLabelKey),
                Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                VerticalAlignment = VerticalAlignment.Center,
                Padding = new Thickness(2, 0, 2, 0),
            };
            Grid.SetColumn(storageHeader, 3);
            headerRow.Children.Add(storageHeader);
            AddSortHeader(4, StorageSortField.FileCount, rightAligned: true);
            AddSortHeader(5, StorageSortField.TotalSize, rightAligned: true);
        }

        // Fetch the first page of storage rows for the active tab/sort plus the
        // outbox snapshot, seed the incremental collection (further pages load
        // as the list scrolls), and refresh the header/footer. Resets the
        // collection wholesale, so a tab or sort change (which changes the
        // server-side row set) calls this again rather than re-rendering cached
        // data — there is no whole-library cache left to re-render from.
        async System.Threading.Tasks.Task LoadStorageRows()
        {
            var (current, firstPageResult) = await _session.RunForCurrentHandle(
                handle => NativeBae.StoragePage(handle, activeTab, sortField, sortDirection, 0, StoragePageSize));
            if (!current)
            {
                return;
            }
            var (page, error) = firstPageResult;
            if (error is not null || page is null)
            {
                storageStatus.Text = error ?? Loc.Chrome("storage.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            // The per-release outbox progress drives both the Uploading tab
            // (server-side now, via the filter) and the Storage cell's upload
            // badge. A failed outbox read leaves the rows without the badge
            // rather than failing the whole list.
            outboxProgress.Clear();
            var (outboxCurrent, outbox) = await _session.RunForCurrentHandle(NativeBae.OutboxSnapshot);
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

            storageStatus.Visibility = Visibility.Collapsed;
            rows.Clear();
            storageCellsByReleaseId.Clear();
            foreach (var row in page.Rows)
            {
                rows.Add(row);
            }
            // Drop selections for releases no longer present after a reload.
            selected.IntersectWith(rows.Select(row => row.Release.Id));
            RefreshRowHighlights();

            var (countCurrent, total) = await _session.RunForCurrentHandle(
                handle => NativeBae.StorageCount(handle, activeTab));
            var totalCount = countCurrent ? checked((ulong)total) : (ulong)page.Rows.Length;
            rows.SeedFirstPage(totalCount, (ulong)page.Rows.Length);

            foreach (var (tab, button) in tabButtons)
            {
                StyleTab(button, tab == activeTab);
            }
            RenderHeader();
            var releasesText = Loc.Chrome("storage.footer.releases", "count", checked((long)totalCount));
            // The core aggregate sums file sizes over every release matching
            // the tab, independent of how many pages have loaded. Shown only
            // once fetched — absence, not a zero/partial stand-in, while a
            // stale session drops the fetch.
            var (sizeCurrent, totalSize) = await _session.RunForCurrentHandle(
                handle => NativeBae.StorageTotalSize(handle, activeTab));
            footer.Text = sizeCurrent
                ? $"{releasesText} · {Loc.Chrome("storage.footer.total", "size", Loc.Bytes(totalSize))}"
                : releasesText;
        }

        await LoadStorageRows();

        // Cloud outbox: the upload/delete queue with a summary band, a Retry-now
        // button, and per-item Cancel. Hidden (empty panel) when nothing is queued.
        // Reloaded after retry/cancel so the panel reflects the new queue state.
        var downloadsPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadDownloads()
        {
            downloadsPanel.Children.Clear();
            var (current, result) = await _session.RunForCurrentHandle(NativeBae.DownloadSnapshot);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            var snapshot = result.Snapshot;
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("storage.read_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            // Hidden when the pin queue is idle, like the outbox panel.
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
                static string DisplayBytes(ulong bytes) =>
                    Loc.Bytes(checked((long)bytes));

                var parts = new List<string>
                {
                    Loc.Chrome("storage.files", "count", op.FileCount),
                    Loc.Bytes(op.TotalSize),
                    StateLabel(op),
                };
                if (DownloadProgress(op.State) is { } progress)
                {
                    parts.Add(
                        Loc.Core(
                            "core.download.bytes_progress",
                            new Dictionary<string, object?>
                            {
                                ["done"] = DisplayBytes(progress.BytesDone),
                                ["total"] = DisplayBytes(progress.BytesTotal),
                            }));
                }
                return string.Join(" · ", parts);
            }

            // Header: a label (or "paused"), Retry (only with failures), and a
            // pause/resume toggle — mirroring the outbox panel's band.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = snapshot.Paused ? Loc.Chrome("download.paused") : Loc.Chrome("download.title"),
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button
            {
                Content = Loc.Chrome("outbox.retry_now"),
                IsEnabled = snapshot.Total.Failed > 0,
            };
            retry.Click += async (_, _) =>
            {
                retry.IsEnabled = false;
                await _session.RunForCurrentHandle(NativeBae.RetryDownloads);
                await LoadDownloads();
            };
            band.Children.Add(retry);
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await _session.RunForCurrentHandle(
                    handle => NativeBae.SetDownloadsPaused(handle, !paused));
                await LoadDownloads();
            };
            band.Children.Add(pause);
            downloadsPanel.Children.Add(band);

            // One row per release: title, "N files · size · state", and a cancel.
            foreach (var op in snapshot.Downloads)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = op.Title, TextWrapping = TextWrapping.Wrap });
                labelColumn.Children.Add(new TextBlock
                {
                    Text = DownloadDetail(op),
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                });
                if (DownloadProgress(op.State) is { } progress)
                {
                    labelColumn.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = 1,
                        Value = progress.Fraction,
                        Height = 4,
                    });
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                var releaseId = op.ReleaseId;
                var cancel = new Button { Content = Loc.Chrome("action.cancel") };
                cancel.Click += async (_, _) =>
                {
                    storageStatus.Visibility = Visibility.Collapsed;
                    cancel.IsEnabled = false;
                    var (cancelCurrent, error) = await _session.RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!cancelCurrent)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
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

        // Exporting: the release-export queue with a summary band (Retry-failed
        // and Pause/Resume) and per-item progress plus Cancel. Hidden (empty
        // panel) when nothing is queued. The queue renders from the snapshot and
        // actions never optimistically mutate — each reload (and the export-queue
        // invalidation) re-renders the section.
        var exportsPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadExports()
        {
            exportsPanel.Children.Clear();
            var (current, snapshot) = await _session.RunForCurrentHandle(NativeBae.ExportSnapshot);
            if (!current)
            {
                return;
            }

            // Hidden when the export queue is idle, like the downloads panel.
            if (snapshot.Exports.Length == 0)
            {
                return;
            }

            string StateLabel(BridgeExportOp op) => op.State switch
            {
                BridgeExportState.Active active => Loc.Chrome(
                    "export.state.exporting", "percent", ExportQueueModel.ClampPercent((int)active.Percent)),
                BridgeExportState.Failed => Loc.Chrome(ExportQueueModel.StateKey(ExportRowKind.Failed)),
                _ => Loc.Chrome(ExportQueueModel.StateKey(ExportRowKind.Queued)),
            };

            // Header: a label (or "paused"), the count summary, Retry (only with
            // failures), and a pause/resume toggle — mirroring the downloads band.
            var paused = snapshot.Paused;
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = Loc.Chrome(ExportQueueModel.BandTitleKey(paused)),
                VerticalAlignment = VerticalAlignment.Center,
            });
            if (!paused && snapshot.SummaryParts.Length > 0)
            {
                band.Children.Add(new TextBlock
                {
                    // Core decides the parts, order, and drop-if-zero; this joins.
                    Text = QueueSummaryText(snapshot.SummaryParts),
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    VerticalAlignment = VerticalAlignment.Center,
                });
            }
            var retry = new Button
            {
                Content = Loc.Chrome("outbox.retry_now"),
                IsEnabled = ExportQueueModel.RetryEnabled(snapshot.Total.Failed),
            };
            retry.Click += async (_, _) =>
            {
                retry.IsEnabled = false;
                await _session.RunForCurrentHandle(NativeBae.RetryExports);
                await LoadExports();
            };
            band.Children.Add(retry);
            var pause = new Button { Content = Loc.Chrome(ExportQueueModel.PauseToggleKey(paused)) };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await _session.RunForCurrentHandle(handle => NativeBae.SetExportsPaused(handle, !paused));
                await LoadExports();
            };
            band.Children.Add(pause);
            exportsPanel.Children.Add(band);

            // One row per export: title, "N files · size · state", an optional
            // progress bar while active, and a cancel.
            foreach (var op in snapshot.Exports)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = op.Title, TextWrapping = TextWrapping.Wrap });
                var detail = new TextBlock
                {
                    Text = $"{Loc.Chrome("storage.files", "count", op.FileCount)} · {Loc.Bytes(op.TotalSize)} · {StateLabel(op)}",
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                };
                // A failed export carries its error on the detail line's tooltip,
                // like the outbox file rows.
                if (op.State is BridgeExportState.Failed failed)
                {
                    ToolTipService.SetToolTip(detail, failed.Error);
                }
                labelColumn.Children.Add(detail);
                if (op.State is BridgeExportState.Active active)
                {
                    labelColumn.Children.Add(new ProgressBar
                    {
                        Minimum = 0,
                        Maximum = 100,
                        Value = ExportQueueModel.ClampPercent((int)active.Percent),
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
                    await _session.RunForCurrentHandle(handle => NativeBae.CancelExport(handle, releaseId));
                    await LoadExports();
                };
                Grid.SetColumn(cancel, 1);
                itemGrid.Children.Add(cancel);
                exportsPanel.Children.Add(itemGrid);
            }
        }

        var outboxPanel = new StackPanel { Spacing = 4 };
        async System.Threading.Tasks.Task LoadOutbox()
        {
            outboxPanel.Children.Clear();
            var (current, result) = await _session.RunForCurrentHandle(NativeBae.OutboxSnapshot);
            if (!current)
            {
                return;
            }
            if (result.Error is not null)
            {
                storageStatus.Text = result.Error;
                storageStatus.Visibility = Visibility.Visible;
                return;
            }
            var snapshot = result.Snapshot;
            if (snapshot is null)
            {
                storageStatus.Text = Loc.Chrome("outbox.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            if (snapshot.UploadGroups.Length == 0 && snapshot.Deletes.Length == 0)
            {
                return;
            }

            // With work queued at least one count is non-zero, so compose the
            // localized queue summary from the generated snapshot counts.
            var band = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            band.Children.Add(new TextBlock
            {
                Text = OutboxSummary(snapshot),
                VerticalAlignment = VerticalAlignment.Center,
            });
            var retry = new Button { Content = Loc.Chrome("outbox.retry_now") };
            retry.Click += async (_, _) =>
            {
                storageStatus.Visibility = Visibility.Collapsed;
                retry.IsEnabled = false;
                var (retryCurrent, error) = await _session.RunForCurrentHandle(NativeBae.RetryOutbox);
                if (!retryCurrent)
                {
                    return;
                }
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                    retry.IsEnabled = true;
                    return;
                }

                await LoadOutbox();
            };
            band.Children.Add(retry);
            // Pause/resume the upload pipeline. Paused leaves items queued but stops
            // the sync cycle from draining them.
            var paused = snapshot.Paused;
            var pause = new Button { Content = paused ? Loc.Chrome("outbox.resume") : Loc.Chrome("outbox.pause") };
            pause.Click += async (_, _) =>
            {
                pause.IsEnabled = false;
                await _session.RunForCurrentHandle(handle => NativeBae.SetSyncPaused(handle, !paused));
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
                var bytesLabel = OutboxBytesLabel(snapshot);
                if (!string.IsNullOrEmpty(bytesLabel))
                {
                    detail.Add(bytesLabel);
                }
                var throughputLabel = OutboxThroughputLabel(snapshot);
                if (!string.IsNullOrEmpty(throughputLabel))
                {
                    detail.Add(throughputLabel);
                }
                var etaLabel = OutboxEtaLabel(snapshot);
                if (!string.IsNullOrEmpty(etaLabel))
                {
                    detail.Add(etaLabel);
                }
                if (detail.Count > 0)
                {
                    outboxPanel.Children.Add(new TextBlock
                    {
                        Text = string.Join(" · ", detail),
                        FontSize = 12,
                        Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    });
                }
            }

            // A queue row: a label (with an optional progress bar), an optional
            // trailing button, and an optional right-click menu.
            void AddOutboxRow(string label, ProgressBar? progress, Button? trailing, MenuFlyout? contextMenu)
            {
                var itemGrid = new Grid { ColumnSpacing = 8 };
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                itemGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                var labelColumn = new StackPanel { Spacing = 2, VerticalAlignment = VerticalAlignment.Center };
                labelColumn.Children.Add(new TextBlock { Text = label, TextWrapping = TextWrapping.Wrap });
                if (progress is not null)
                {
                    labelColumn.Children.Add(progress);
                }
                Grid.SetColumn(labelColumn, 0);
                itemGrid.Children.Add(labelColumn);

                if (trailing is not null)
                {
                    Grid.SetColumn(trailing, 1);
                    itemGrid.Children.Add(trailing);
                }
                if (contextMenu is not null)
                {
                    itemGrid.ContextFlyout = contextMenu;
                }
                outboxPanel.Children.Add(itemGrid);
            }

            // Runs `action` off-thread, surfaces any error to the status line, and
            // reloads the panel on success — shared by the row button and menu.
            async System.Threading.Tasks.Task RunCancel(Func<AppHandle, string?> action)
            {
                storageStatus.Visibility = Visibility.Collapsed;
                var (current, error) = await _session.RunForCurrentHandle(action);
                if (!current)
                {
                    return;
                }
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                    return;
                }

                await LoadOutbox();
            }

            // A right-click "Cancel" menu, matching the storage table's per-release
            // cancel. Used for the upload release rows.
            MenuFlyout CancelFlyout(Func<AppHandle, string?> action)
            {
                var menu = new MenuFlyout();
                var item = new MenuFlyoutItem { Text = Loc.Chrome("action.cancel") };
                item.Click += async (_, _) => await RunCancel(action);
                menu.Items.Add(item);
                return menu;
            }

            // Uploads: one expandable row per release (matching the storage
            // table and the macOS queue pane) — title, file count, cumulative
            // byte progress with a bar, an aggregate state badge, and the
            // per-file list inside. Right-click cancels the release's
            // transition; the orphaned-files bucket (no release id) has no
            // release to cancel.
            foreach (var group in snapshot.UploadGroups)
            {
                var header = new StackPanel { Spacing = 2 };
                var titleLine = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                titleLine.Children.Add(new TextBlock
                {
                    Text = group.DisplayTitle,
                    VerticalAlignment = VerticalAlignment.Center,
                });
                titleLine.Children.Add(new TextBlock
                {
                    Text = $"{Loc.Chrome("storage.files", "count", group.Files.Length)} · {UploadBytesLabel(group.Progress)}",
                    FontSize = 12,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    VerticalAlignment = VerticalAlignment.Center,
                });
                titleLine.Children.Add(new TextBlock
                {
                    Text = UploadBadgeLabel(group.Progress),
                    FontSize = 12,
                    Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                    VerticalAlignment = VerticalAlignment.Center,
                });
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
                    var fileGrid = new Grid { ColumnSpacing = 8 };
                    fileGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
                    fileGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });

                    var fileColumn = new StackPanel { Spacing = 2 };
                    fileColumn.Children.Add(new TextBlock
                    {
                        Text = $"{file.DisplayName} · {FileBytesLabel(file)}",
                        FontSize = 12,
                        TextWrapping = TextWrapping.Wrap,
                    });
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

                    var state = new TextBlock
                    {
                        Text = FileStateLabel(file.State),
                        FontSize = 12,
                        Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                        VerticalAlignment = VerticalAlignment.Center,
                    };
                    if (file.State == BridgeUploadFileState.Retrying && file.LastError is string lastError)
                    {
                        ToolTipService.SetToolTip(state, lastError);
                    }
                    Grid.SetColumn(state, 1);
                    fileGrid.Children.Add(state);
                    filesPanel.Children.Add(fileGrid);
                }

                var expander = new Expander
                {
                    Header = header,
                    Content = filesPanel,
                    // Expanded by default: the per-file list is the pane's point.
                    IsExpanded = true,
                    HorizontalAlignment = HorizontalAlignment.Stretch,
                    HorizontalContentAlignment = HorizontalAlignment.Stretch,
                };
                if (group.ReleaseId is string releaseId)
                {
                    expander.ContextFlyout = CancelFlyout(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                }
                outboxPanel.Children.Add(expander);
            }
            // A pending delete is genuinely a single-file operation, so it keeps
            // its own per-file cancel button.
            foreach (var delete in snapshot.Deletes)
            {
                var cancel = new Button { Content = Loc.Chrome("outbox.cancel_item") };
                var id = delete.Id;
                cancel.Click += async (_, _) => await RunCancel(
                    handle => NativeBae.CancelOutboxItem(handle, id));
                AddOutboxRow(DeleteLabel(delete), null, trailing: cancel, contextMenu: null);
            }
        }

        await LoadDownloads();
        await LoadExports();
        await LoadOutbox();

        // Device-local transfer concurrency (1..8). Always visible, unlike the
        // queue panels that hide when idle; desktop configures both directions.
        // SelectedIndex is seeded before wiring SelectionChanged so seeding does
        // not fire a spurious write. bae-core carries the value, not the bound.
        ComboBox MakeConcurrencyPicker(string header, uint current, Func<AppHandle, uint, string?> apply)
        {
            var combo = new ComboBox { Header = header };
            for (uint i = 1; i <= 8; i++)
            {
                combo.Items.Add(new ComboBoxItem { Content = i.ToString(), Tag = i });
            }
            combo.SelectedIndex = (int)current - 1;
            combo.SelectionChanged += async (_, _) =>
            {
                if (combo.SelectedItem is ComboBoxItem item && item.Tag is uint n)
                {
                    var (_, error) = await _session.RunForCurrentHandle(handle => apply(handle, n));
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                    }
                }
            };
            return combo;
        }

        var transferPanel = new StackPanel { Spacing = 4 };
        var (_, transferConfig) = await _session.RunForCurrentHandle(NativeBae.GetConfig);
        if (transferConfig is { } cfg)
        {
            transferPanel.Children.Add(MakeConcurrencyPicker(
                "Simultaneous downloads", cfg.MaxConcurrentDownloads, NativeBae.SetMaxConcurrentDownloads));
            transferPanel.Children.Add(MakeConcurrencyPicker(
                "Simultaneous uploads", cfg.MaxConcurrentUploads, NativeBae.SetMaxConcurrentUploads));
        }

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(storageStatus);
        content.Children.Add(transferPanel);
        content.Children.Add(downloadsPanel);
        content.Children.Add(exportsPanel);
        content.Children.Add(outboxPanel);
        content.Children.Add(tabBar);
        content.Children.Add(headerRow);
        content.Children.Add(list);
        content.Children.Add(footer);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("storage.title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 480 },
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };
        // Refresh the panels live while the dialog is open as uploads/deletes/pins
        // progress; the registrations are disposed on close. An album/release
        // invalidation that isn't an outbox change can still alter a release's
        // storage state, so it refreshes the rows too.
        var registrations = new List<IDisposable>
        {
            _projections.Register(typeof(BridgeInvalidation.Outbox), () =>
            {
                _ = LoadOutbox();
                _ = LoadStorageRows();
            }),
            _projections.Register(typeof(BridgeInvalidation.DownloadQueue), () =>
            {
                _ = LoadDownloads();
                _ = LoadStorageRows();
            }),
            _projections.Register(typeof(BridgeInvalidation.ExportQueue), () => _ = LoadExports()),
            _projections.Register(typeof(BridgeInvalidation.Album), () => _ = LoadStorageRows()),
            _projections.Register(typeof(BridgeInvalidation.Release), () => _ = LoadStorageRows()),
        };
        // A transfer starting or ending refreshes the loaded rows' Storage cells
        // (it shows the in-flight verb); the Release invalidation core emits
        // alongside it refetches the snapshot.
        void OnTransfersChanged() => RefreshStorageCells();
        _transfers.Changed += OnTransfersChanged;
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            _transfers.Changed -= OnTransfersChanged;
            foreach (var registration in registrations)
            {
                registration.Dispose();
            }
        }
    }

    // Build the right-tap menu for the targeted releases: the intersected storage
    // transitions plus a cancel for any of their queued uploads. Each item runs the
    // action on every targeted release, then reloads the rows.
    private async System.Threading.Tasks.Task<MenuFlyout> BuildStorageRowMenu(
        List<string> releaseIds,
        Dictionary<string, BridgeStorageRow> rowsById,
        TextBlock storageStatus,
        Func<System.Threading.Tasks.Task> reload)
    {
        var menu = new MenuFlyout();

        // A release with a transition in flight offers only "Cancel" — the
        // storage actions (pin/unmanage/…) would race it. Each transition
        // surfaces differently: an upload sits in the outbox snapshot, a pin in
        // the download queue snapshot, an unmanage (a blocking foreground
        // transfer with no queue) is tracked in the store while it runs, and any
        // event-reported transfer is in the overlay. Core dispatches to whichever
        // is running.
        var (transitioning, uploadError) = await _storage.TransitioningReleases(releaseIds);
        if (uploadError is not null)
        {
            storageStatus.Text = uploadError;
            storageStatus.Visibility = Visibility.Visible;
        }
        if (transitioning.Count > 0)
        {
            var cancel = new MenuFlyoutItem { Text = Loc.Chrome("action.cancel") };
            cancel.Click += async (_, _) =>
            {
                foreach (var releaseId in transitioning)
                {
                    var (current, error) = await _session.RunForCurrentHandle(
                        handle => NativeBae.CancelReleaseTransition(handle, releaseId));
                    if (!current)
                    {
                        return;
                    }
                    if (error is not null)
                    {
                        storageStatus.Text = error;
                        storageStatus.Visibility = Visibility.Visible;
                        return;
                    }
                }

                await reload();
            };
            menu.Items.Add(cancel);
            return menu;
        }

        foreach (var action in _storage.IntersectedStorageActions(releaseIds, rowsById))
        {
            var act = action;
            var item = new MenuFlyoutItem { Text = DialogPrimitives.StorageActionLabel(act) };
            item.Click += async (_, _) =>
            {
                var error = await _storage.RunStorageActionForReleases(
                    act, releaseIds, () => DialogPrimitives.PickUnmanageFolder(_windowHandle()));
                if (error is not null)
                {
                    storageStatus.Text = error;
                    storageStatus.Visibility = Visibility.Visible;
                }
                else
                {
                    await reload();
                }
            };
            menu.Items.Add(item);
        }

        return menu;
    }

    // Selected-row highlight: a faint accent tint, or transparent when not
    // selected. Static so LoadStorageRows and RefreshRowHighlights agree.
    private static Brush RowBackground(bool isSelected) =>
        isSelected
            ? new SolidColorBrush(Microsoft.UI.Colors.SteelBlue) { Opacity = 0.25 }
            : new SolidColorBrush(Microsoft.UI.Colors.Transparent);

    // The six-column grid shared by the header and each row: Album and Artist
    // stretch, Format / Storage auto-size, Files and Size auto-size (right-
    // aligned in the cell).
    private static Grid MakeStorageGrid()
    {
        var grid = new Grid { ColumnSpacing = 8 };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(2, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(2, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        return grid;
    }

    private static TextBlock AddCell(Grid grid, int column, string text, bool rightAligned = false)
    {
        var block = new TextBlock
        {
            Text = text,
            VerticalAlignment = VerticalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
            HorizontalAlignment = rightAligned ? HorizontalAlignment.Right : HorizontalAlignment.Left,
        };
        Grid.SetColumn(block, column);
        grid.Children.Add(block);
        return block;
    }

    // Emphasize the active tab and gray the inactive ones, matching the import
    // dialog's tab bar.
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

    private static BridgeDownloadTransferProgress? DownloadProgress(BridgeDownloadState state) =>
        state is BridgeDownloadState.Active active ? active.Progress : null;

    // Core decides the outbox summary's parts (uploading/failed/queued/pending
    // deletes), their order, and the drop-if-zero rule; this only localizes and
    // joins them.
    private static string OutboxSummary(BridgeOutboxSnapshot snapshot) =>
        QueueSummaryText(snapshot.SummaryParts);

    // Render core's queue-summary parts into one " · "-joined line.
    private static string QueueSummaryText(IReadOnlyList<BridgeCountLabel> parts) =>
        string.Join(" · ", parts.Select(part => Loc.Core(part.Key, "count", part.Count)));

    private static string OutboxThroughputLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.ThroughputBps > 0
            ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(checked((long)snapshot.ThroughputBps)))
            : string.Empty;

    private static string OutboxEtaLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.EtaSeconds is { } seconds
            ? Loc.Core("core.outbox.eta", "duration", BridgeDisplay.Clock(checked(checked((long)seconds) * 1000)))
            : string.Empty;

    private static string OutboxBytesLabel(BridgeOutboxSnapshot snapshot)
    {
        if (snapshot.Total.BytesTotal == 0) return string.Empty;
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)snapshot.Total.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)snapshot.Total.BytesTotal)),
            });
    }

    // A release group's aggregate badge, mirroring the macOS queue pane: the
    // dominant activity plus the unshipped file count. Finished releases
    // aren't rendered, so there is no terminal badge. Empty for a group core
    // would never emit (no activity).
    private static string UploadBadgeLabel(BridgeUploadProgress progress)
    {
        var pending = progress.Queued + progress.Active + progress.Failed;
        return progress.Activity switch
        {
            BridgeUploadActivity.Uploading => Loc.Chrome("outbox.badge.uploading", "count", pending),
            BridgeUploadActivity.Queued => Loc.Chrome("outbox.badge.queued", "count", pending),
            BridgeUploadActivity.Retrying => Loc.Chrome("outbox.badge.retrying", "count", pending),
            _ => string.Empty,
        };
    }

    // A release group's byte progress: "45.2 MB of 103.1 MB", cumulative over
    // the queue burst, matching the bar beside it.
    private static string UploadBytesLabel(BridgeUploadProgress progress)
    {
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)progress.BytesDone)),
                ["total"] = Loc.Bytes(checked((long)progress.BytesTotal)),
            });
    }

    private static string FileStateLabel(BridgeUploadFileState state) => state switch
    {
        BridgeUploadFileState.Uploading => Loc.Chrome("outbox.state.uploading"),
        BridgeUploadFileState.Queued => Loc.Chrome("outbox.state.queued"),
        BridgeUploadFileState.Retrying => Loc.Chrome("outbox.state.retrying"),
        BridgeUploadFileState.Done => Loc.Chrome("outbox.state.done"),
        _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown upload file state"),
    };

    // A file's byte text: "6.2 MB of 12.4 MB" while transferring; just the
    // size otherwise.
    private static string FileBytesLabel(BridgeUploadFileOp file)
    {
        var total = Loc.Bytes(checked((long)file.BytesTotal));
        if (file.State != BridgeUploadFileState.Uploading) return total;
        return Loc.Core(
            "core.outbox.bytes_progress",
            new Dictionary<string, object?>
            {
                ["done"] = Loc.Bytes(checked((long)file.BytesDone)),
                ["total"] = total,
            });
    }

    private static string DeleteLabel(BridgeDeleteOp delete) =>
        $"{delete.CloudKey} — {Loc.Chrome("outbox.delete.kind")}";
}
