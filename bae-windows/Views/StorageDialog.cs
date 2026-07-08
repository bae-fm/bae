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
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Func<IntPtr> _windowHandle;
    private readonly StorageStore _storage;
    private readonly ProjectionRegistry _projections;

    public StorageDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        Func<IntPtr> windowHandle,
        StorageStore storage,
        ProjectionRegistry projections)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _windowHandle = windowHandle;
        _storage = storage;
        _projections = projections;
    }

    public async System.Threading.Tasks.Task Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        var listPanel = new StackPanel { Spacing = 4, MinWidth = 460 };
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
        // The current rows, kept so the right-tap menu can resolve a release's
        // allowed actions (for the multi-select intersection) by id.
        var rowsById = new Dictionary<string, BridgeStorageRow>();

        // Each row shows its summary; a left-click toggles its selection and a
        // right-click opens a menu of the transitions the core says it allows
        // (carried on the row, gated on cloud-home + pending uploads), plus
        // cancel for any queued uploads. The same actions run on every selected
        // release.
        async System.Threading.Tasks.Task LoadStorageRows()
        {
            var (current, result) = await _session.RunForCurrentHandle(NativeBae.StorageRows);
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
            if (result.Rows is null)
            {
                storageStatus.Text = Loc.Chrome("storage.load_failed");
                storageStatus.Visibility = Visibility.Visible;
                return;
            }

            storageStatus.Visibility = Visibility.Collapsed;
            rowsById.Clear();
            foreach (var row in result.Rows)
            {
                rowsById[row.Release.Id] = row;
            }
            // Drop selections for releases no longer present after a transition.
            selected.IntersectWith(rowsById.Keys);

            listPanel.Children.Clear();
            foreach (var row in result.Rows)
            {
                var text = new TextBlock
                {
                    Text = StorageRowSummary(row),
                    VerticalAlignment = VerticalAlignment.Center,
                    TextWrapping = TextWrapping.Wrap,
                };
                var releaseId = row.Release.Id;
                var rowBorder = new Border
                {
                    Child = text,
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

                    var menu = await BuildStorageRowMenu(
                        selected.ToList(), rowsById, storageStatus, LoadStorageRows);
                    // Nothing to offer (e.g. no cloud home, or uploads in flight)
                    // — skip the empty popup.
                    if (menu.Items.Count > 0)
                    {
                        menu.ShowAt(rowBorder, new FlyoutShowOptions { Position = position });
                    }
                };

                listPanel.Children.Add(rowBorder);
            }

            void RefreshRowHighlights()
            {
                foreach (var child in listPanel.Children)
                {
                    if (child is Border border && border.Tag is string id)
                    {
                        border.Background = RowBackground(selected.Contains(id));
                    }
                }
            }
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
        await LoadOutbox();

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(storageStatus);
        content.Children.Add(downloadsPanel);
        content.Children.Add(outboxPanel);
        content.Children.Add(listPanel);

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
            _projections.Register(typeof(BridgeInvalidation.Album), () => _ = LoadStorageRows()),
            _projections.Register(typeof(BridgeInvalidation.Release), () => _ = LoadStorageRows()),
        };
        try
        {
            await dialog.ShowAsync();
        }
        finally
        {
            foreach (var registration in registrations)
            {
                registration.Dispose();
            }
        }
    }

    // The folder picker for a make-local (unmanage) transition, run in the app
    // window. Returns the chosen path, or null when the user cancelled.
    private async System.Threading.Tasks.Task<string?> PickUnmanageFolder()
    {
        var picker = new global::Windows.Storage.Pickers.FolderPicker();
        picker.FileTypeFilter.Add("*");
        WinRT.Interop.InitializeWithWindow.Initialize(picker, _windowHandle());
        var folder = await picker.PickSingleFolderAsync();
        return folder?.Path;
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
        // the download queue snapshot, and an unmanage (a blocking foreground
        // transfer with no queue) is tracked in the store while it runs. Core
        // dispatches to whichever is running.
        var (uploading, uploadError) = await _storage.UploadingReleases(releaseIds);
        if (uploadError is not null)
        {
            storageStatus.Text = uploadError;
            storageStatus.Visibility = Visibility.Visible;
        }
        var transitioning = new HashSet<string>(uploading);
        transitioning.UnionWith(await _storage.DownloadingReleases(releaseIds));
        transitioning.UnionWith(releaseIds.Where(_storage.IsUnmanaging));
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
            var item = new MenuFlyoutItem { Text = StorageActionLabel(act) };
            item.Click += async (_, _) =>
            {
                var error = await _storage.RunStorageActionForReleases(act, releaseIds, PickUnmanageFolder);
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

    // User-facing label for a storage transition, matching the macOS
    // "Storage…" sheet / context menu wording.
    private static string StorageRowSummary(BridgeStorageRow row)
    {
        var format = string.IsNullOrEmpty(row.Release.Format) ? string.Empty : $" · {row.Release.Format}";
        var files = Loc.Chrome("storage.files", "count", row.Release.FileCount);
        return $"{row.Album.Title} — {row.Album.ArtistNames}{format} · {files} · {Loc.Bytes(row.Release.TotalSize)} · {StorageStateLabel(row.Release.StorageState)}{PinIndicator(row.Release)}";
    }

    private static string StorageStateLabel(BridgeReleaseStorageState state) => state switch
    {
        BridgeReleaseStorageState.Remote => Loc.Chrome("storage.state.managed"),
        BridgeReleaseStorageState.Local => Loc.Chrome("storage.state.unmanaged"),
        _ => throw new ArgumentOutOfRangeException(nameof(state), state, "Unknown storage state"),
    };

    private static string PinIndicator(BridgeReleaseSummary release) =>
        release.Pinned ? $" · {Loc.Chrome("storage.pinned")}" : string.Empty;

    private static BridgeDownloadTransferProgress? DownloadProgress(BridgeDownloadState state) =>
        state is BridgeDownloadState.Active active ? active.Progress : null;

    private static string OutboxSummary(BridgeOutboxSnapshot snapshot)
    {
        var parts = new List<string>();
        if (snapshot.Total.Active > 0) parts.Add(Loc.Core("core.queue.uploading", "count", snapshot.Total.Active));
        if (snapshot.Total.Failed > 0) parts.Add(Loc.Core("core.queue.failed", "count", snapshot.Total.Failed));
        if (snapshot.Total.Queued > 0) parts.Add(Loc.Core("core.queue.queued", "count", snapshot.Total.Queued));
        if (snapshot.Deletes.Length > 0)
            parts.Add(Loc.Core("core.outbox.pending_deletes", "count", snapshot.Deletes.Length));
        return string.Join(" · ", parts);
    }

    private static string OutboxThroughputLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.ThroughputBps > 0
            ? Loc.Core("core.outbox.throughput", "rate", Loc.Bytes(checked((long)snapshot.ThroughputBps)))
            : string.Empty;

    private static string OutboxEtaLabel(BridgeOutboxSnapshot snapshot) =>
        snapshot.EtaSeconds is { } seconds
            ? Loc.Core("core.outbox.eta", "duration", Loc.Duration(checked(checked((long)seconds) * 1000)))
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

    private static string StorageActionLabel(BridgeReleaseStorageAction action) => action switch
    {
        BridgeReleaseStorageAction.MakeRemote => Loc.Chrome("storage.action.manage"),
        BridgeReleaseStorageAction.MakeLocal => Loc.Chrome("storage.action.unmanage"),
        BridgeReleaseStorageAction.Pin => Loc.Chrome("storage.action.pin"),
        BridgeReleaseStorageAction.Unpin => Loc.Chrome("storage.action.unpin"),
        _ => throw new ArgumentOutOfRangeException(nameof(action), action, "Unknown storage action"),
    };
}
