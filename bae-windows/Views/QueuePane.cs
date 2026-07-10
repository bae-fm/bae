using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;

namespace Bae.Windows;

// The queue side pane: a non-modal surface hosting the manual "Up Next" lane and
// the playback context as ONE virtualized, reorderable ListView (never a ListView
// inside a StackPanel inside a ScrollViewer — that combination hands the ListView
// unbounded height, so ItemsStackPanel realizes every row instead of
// virtualizing). Non-modal so the album grid behind it can source a drag while
// the pane stays open — album cards drop into the manual lane at a chosen index.
// Reads the lanes from the playback store and rebuilds on its QueueChanged while
// visible; each row skips on click, removes on right-tap, and reorders on drag
// (forwarded to core by entry id). Clear empties only the manual lane. External
// drops land only in the manual lane; the context lane rejects them. The context
// lane is library-scaled and only partly resolved: it pages further entries in via
// ISupportIncrementalLoading as the list scrolls toward its end, revision-checked
// against the store so a reply for a superseded queue state is dropped.
internal sealed class QueuePane
{
    private readonly SessionStore _session;
    private readonly PlaybackStore _playback;
    private readonly Border _host;
    private readonly Action<string> _onError;

    public QueuePane(SessionStore session, PlaybackStore playback, Border host, Action<string> onError)
    {
        _session = session;
        _playback = playback;
        _host = host;
        _onError = onError;
    }

    // -- Row model ---------------------------------------------------------
    //
    // The pane's single ListView is bound to one flat, heterogeneous row
    // collection. Every row carries the lane it belongs to; the collection's
    // rows are always laid out as contiguous same-lane runs in `Lane` ordinal
    // order (Chrome, then Manual, then Context) — reorder validation leans on
    // that invariant instead of tracking section boundaries separately.
    private enum QueueLane
    {
        Chrome,
        Manual,
        Context,
    }

    private abstract record QueueRow(QueueLane Lane);

    // The "Clear" action, at the top of the scrollable content (unchanged from
    // today's layout — it scrolls with the list rather than becoming pane chrome,
    // matching the current visual order).
    private sealed record ClearRow() : QueueRow(QueueLane.Chrome);

    // A section header. Shuffled is non-null only for the context section (the
    // manual lane is never shuffled and has no toggle).
    private sealed record SectionHeaderRow(QueueLane Lane, string Text, bool? Shuffled) : QueueRow(Lane);

    // One queue row's display: the entry, its id, and the one-line summary.
    private sealed record EntryRow(QueueLane Lane, BridgeQueueEntry Entry) : QueueRow(Lane)
    {
        internal string EntryId => Entry.EntryId;
        public override string ToString() =>
            $"{Entry.Title} — {Entry.ArtistNames} · {Loc.Duration(Entry.DurationMs)}".Trim();
    }

    // The empty manual-lane state: shown instead of any manual EntryRow when the
    // lane is empty, so a drop always has a target at the front of the pane.
    private sealed record EmptyManualRow() : QueueRow(QueueLane.Manual);

    // A trailing drop strip appended after a non-empty manual lane, mirroring the
    // reference platforms' "drop here to append" zone. Belongs to Manual — it
    // sits inside that lane's contiguous run, immediately before the context
    // section (or the end of the list).
    private sealed record TrailingDropRow() : QueueRow(QueueLane.Manual);

    public bool IsOpen => _host.Visibility == Visibility.Visible;

    // Show the pane if hidden, hide it if shown. Bound to the queue button and its
    // Ctrl+Shift+Q accelerator.
    public void Toggle()
    {
        if (IsOpen)
        {
            Hide();
        }
        else
        {
            Show();
        }
    }

    public void Hide()
    {
        if (!IsOpen)
        {
            return;
        }
        _playback.QueueChanged -= Rebuild;
        _host.Child = null;
        _host.Visibility = Visibility.Collapsed;
    }

    private void Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }
        _playback.QueueChanged += Rebuild;
        Rebuild();
        _host.Visibility = Visibility.Visible;
    }

    // Rebuild the pane's content from the current lanes. Safe to run mid-drag:
    // core emits QueueChanged only after a mutation lands, and the whole content
    // is rebuilt between events (the modal dialog it replaced did the same per
    // opening). QueueChanged is also the invalidation signal for any in-flight
    // incremental-load page fetch on the PREVIOUS collection instance: this
    // always constructs a fresh QueuePaneRowCollection, so a stale reply lands on
    // an abandoned instance nobody renders.
    private void Rebuild()
    {
        var manual = _playback.ManualQueue;
        var context = _playback.Context;
        var revision = _playback.Revision;

        var rows = new QueuePaneRowCollection(_session, revision, contextTotal: context?.UpcomingTotal ?? 0);

        rows.Add(new ClearRow());
        rows.Add(new SectionHeaderRow(QueueLane.Manual, Loc.Chrome("queue.section.up_next"), Shuffled: null));
        if (manual.Count == 0)
        {
            rows.Add(new EmptyManualRow());
        }
        else
        {
            foreach (var entry in manual)
            {
                rows.Add(new EntryRow(QueueLane.Manual, entry));
            }
            rows.Add(new TrailingDropRow());
        }

        if (context is { UpcomingTotal: > 0 } ctx)
        {
            // The context section names what's playing — a release ("Playing From")
            // vs the whole library — by the source kind the wire shape carries.
            var labelKey = ctx.Kind == BridgePlaybackSourceKind.Library
                ? "queue.section.your_library"
                : "queue.section.playing_from";
            rows.Add(new SectionHeaderRow(QueueLane.Context, Loc.Chrome(labelKey), ctx.Shuffled));
            foreach (var entry in ctx.Upcoming)
            {
                rows.Add(new EntryRow(QueueLane.Context, entry));
            }
            rows.MarkInitialContextLoad((ulong)ctx.Upcoming.Length);
        }

        var list = BuildQueueList(rows, manual);

        var content = new Grid();
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var header = BuildHeader();
        Grid.SetRow(header, 0);
        content.Children.Add(header);

        Grid.SetRow(list, 1);
        content.Children.Add(list);

        _host.Child = content;
    }

    // The pane header: the "queue" title and a close button that hides the pane.
    private Grid BuildHeader()
    {
        var title = new TextBlock
        {
            Text = Loc.Chrome("queue.title"),
            Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"],
            VerticalAlignment = VerticalAlignment.Center,
        };
        var close = new Button
        {
            // Segoe MDL2 Assets "ChromeClose" glyph (U+E711).
            Content = new FontIcon { Glyph = "", FontSize = 14 },
            HorizontalAlignment = HorizontalAlignment.Right,
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(close, Loc.Chrome("action.close"));
        ToolTipService.SetToolTip(close, Loc.Chrome("action.close"));
        close.Click += (_, _) => Hide();

        var grid = new Grid { Padding = new Thickness(16, 12, 12, 12) };
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        grid.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(title, 0);
        Grid.SetColumn(close, 1);
        grid.Children.Add(title);
        grid.Children.Add(close);
        return grid;
    }

    // The single virtualized list hosting every row: chrome, both section
    // headers, and both lanes' entries. Fills the pane's remaining height (the
    // containing Grid row is Star-sized) so it is its own bounded scroll region —
    // there is no outer ScrollViewer.
    private ListView BuildQueueList(QueuePaneRowCollection rows, IReadOnlyList<BridgeQueueEntry> manual)
    {
        var list = new ListView
        {
            ItemsSource = rows,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            CanReorderItems = true,
            CanDragItems = true,
            AllowDrop = true,
            Padding = new Thickness(16, 8, 16, 16),
            IncrementalLoadingTrigger = IncrementalLoadingTrigger.Edge,
        };

        list.ContainerContentChanging += (_, args) =>
        {
            if (args.InRecycleQueue)
            {
                return;
            }
            args.ItemContainer.Content = BuildRowVisual(args.Item as QueueRow);
            args.Handled = true;
        };

        // Only an EntryRow is draggable — headers, the empty state, and the
        // trailing drop strip are chrome, not queue content.
        list.DragItemsStarting += (_, args) =>
        {
            if (args.Items.Any(item => item is not EntryRow))
            {
                args.Cancel = true;
            }
        };

        list.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is EntryRow clicked)
            {
                _session.WithCurrentHandle(handle => NativeBae.QueueSkipTo(handle, clicked.EntryId));
            }
        };

        // Right-tap a row to drop it from the queue. Removing locally too keeps
        // the pane in sync; the Move-only reorder handler below ignores this
        // Remove (it only reacts to NotifyCollectionChangedAction.Move).
        list.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element
                || element.DataContext is not EntryRow item)
            {
                return;
            }

            var index = rows.IndexOf(item);
            if (index < 0)
            {
                return;
            }

            var menu = new MenuFlyout();
            var remove = new MenuFlyoutItem { Text = Loc.Chrome("queue.remove_item") };
            remove.Click += (_, _) =>
            {
                // The generated bridge removes by entry id, so a reorder between the right-tap and
                // the click can't target the wrong row. The local index is only to keep
                // the pane's collection in sync.
                var idx = rows.IndexOf(item);
                if (idx < 0)
                {
                    return;
                }
                _session.WithCurrentHandle(handle => NativeBae.QueueRemove(handle, item.EntryId));
                rows.RemoveAt(idx);
            };
            menu.Items.Add(remove);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        // A drag-reorder mutates `rows` in place before this fires (the framework's
        // own behavior); validate it stayed within one lane before forwarding to
        // core, and revert (a full Rebuild, since core's state never changed) if
        // it crossed a section boundary — dragging past a header is visually
        // possible but not a valid move.
        rows.CollectionChanged += (_, args) =>
        {
            if (args.Action != NotifyCollectionChangedAction.Move)
            {
                return;
            }
            if (rows[args.NewStartingIndex] is not EntryRow moved)
            {
                return;
            }
            if (!IsLaneOrderValid(rows))
            {
                BaeDiagnostics.Logger.Warning(
                    $"Queue reorder for {moved.EntryId} crossed a section boundary; reverting.");
                Rebuild();
                return;
            }

            var laneEntryIds = rows.OfType<EntryRow>().Where(e => e.Lane == moved.Lane)
                .Select(e => e.EntryId).ToList();
            var newIndex = laneEntryIds.IndexOf(moved.EntryId);
            var move = QueueReorderModel.ResolveMove(laneEntryIds, newIndex);
            _session.WithCurrentHandle(handle => NativeBae.QueueReorder(handle, move.MovedEntryId, move.BeforeEntryId));
        };

        // External album-card drops land only in the manual lane; the pointer's Y
        // position resolves to an insert index among the manual lane's realized
        // rows (or the lane's end when it's empty or the pointer is past them).
        AttachExternalDrop(list, e => ComputeInsertIndex(list, rows, manual, e));

        return list;
    }

    // Whether `rows` still partitions into contiguous same-lane runs in
    // Chrome/Manual/Context ordinal order — the invariant a valid single-lane
    // reorder always preserves. A cross-lane drag breaks it (some row's ordinal
    // would regress relative to an earlier row's).
    private static bool IsLaneOrderValid(IReadOnlyList<QueueRow> rows)
    {
        var lastOrdinal = -1;
        foreach (var row in rows)
        {
            var ordinal = (int)row.Lane;
            if (ordinal < lastOrdinal)
            {
                return false;
            }
            lastOrdinal = ordinal;
        }
        return true;
    }

    // Build one row's visual by kind. ContainerContentChanging re-fires per
    // recycled container, so this runs again whenever a row's content changes
    // (including a fresh page landing).
    private FrameworkElement BuildRowVisual(QueueRow? row) => row switch
    {
        ClearRow => BuildClearRow(),
        SectionHeaderRow { Shuffled: null } header => QueueSectionLabel(header.Text),
        SectionHeaderRow header => ContextSectionLabel(header.Text, header.Shuffled!.Value),
        EntryRow entry => BuildEntryRow(entry),
        EmptyManualRow => BuildEmptyDropArea(),
        TrailingDropRow => BuildTrailingDropArea(),
        _ => throw new ArgumentOutOfRangeException(nameof(row), row, "Unknown queue row kind"),
    };

    // Clear empties only the manual lane (the context survives), so it disables
    // on an empty manual lane regardless of the context.
    private Button BuildClearRow()
    {
        var clear = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            IsEnabled = _playback.ManualQueue.Count > 0,
        };
        clear.Click += (_, _) => _session.WithCurrentHandle(NativeBae.QueueClear);
        return clear;
    }

    // The empty manual-lane state: a heading and a hint inside a drop-accepting
    // card; a drop lands at the front of the (empty) lane.
    private Border BuildEmptyDropArea()
    {
        var stack = new StackPanel
        {
            Spacing = 4,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
        };
        stack.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("queue.empty.title"),
            HorizontalAlignment = HorizontalAlignment.Center,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        stack.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("queue.empty.hint"),
            HorizontalAlignment = HorizontalAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
            TextAlignment = TextAlignment.Center,
            Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        });
        return new Border
        {
            MinHeight = 96,
            Padding = new Thickness(16),
            CornerRadius = new CornerRadius(6),
            Background = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = stack,
        };
    }

    // A drop strip below the manual lane's rows that appends at the lane's end,
    // mirroring the reference platforms' trailing zone.
    private static Border BuildTrailingDropArea() => new()
    {
        Height = 40,
        Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Transparent),
    };

    // A plain section header for the queue pane (the manual "Up Next" lane, which
    // is never shuffled and has no shuffle control).
    private static TextBlock QueueSectionLabel(string text) => new()
    {
        Text = text,
        FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
    };

    // The context section's header, with a shuffle toggle that flips the context
    // between sequential and shuffled order while the current track keeps playing.
    private StackPanel ContextSectionLabel(string text, bool shuffled)
    {
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 6,
            VerticalAlignment = VerticalAlignment.Center,
        };
        row.Children.Add(new TextBlock
        {
            Text = text,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            VerticalAlignment = VerticalAlignment.Center,
        });
        // Segoe MDL2 Assets "Shuffle" glyph (U+E8B1), accented when on.
        var toggle = new Button
        {
            Content = new FontIcon
            {
                Glyph = "",
                FontSize = 14,
                Foreground = shuffled
                    ? (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["AccentTextFillColorPrimaryBrush"]
                    : (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
            },
            Padding = new Thickness(6, 2, 6, 2),
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(
            toggle, Loc.Chrome(shuffled ? "queue.shuffle.off" : "queue.shuffle.on"));
        ToolTipService.SetToolTip(
            toggle, Loc.Chrome(shuffled ? "queue.shuffle.off" : "queue.shuffle.on"));
        toggle.Click += (_, _) => _session.WithCurrentHandle(
            handle => NativeBae.SetShuffle(handle, !shuffled));
        row.Children.Add(toggle);
        return row;
    }

    private static Grid BuildEntryRow(EntryRow row)
    {
        // The row's own visual is unchanged from the reference implementation;
        // wrapping it lets ContainerContentChanging swap content per row kind
        // while keeping ListView's own drag/reorder/right-tap plumbing above.
        var grid = new Grid();
        var text = new TextBlock
        {
            Text = row.ToString(),
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
            Padding = new Thickness(0, 8, 0, 8),
        };
        grid.Children.Add(text);
        return grid;
    }

    // Wire a surface to accept an external album-card drop into the manual lane.
    // The Text discriminator separates external drags (which carry the album-id
    // payload) from the framework's internal reorder (which carries no Text), so
    // reorder is left untouched. The caption previews the 1-based target position.
    // e.Handled is required: DragOver/Drop bubble, and the window's folder-drop
    // handler would otherwise overwrite the accepted operation for a non-file drag.
    private void AttachExternalDrop(UIElement target, Func<DragEventArgs, int> computeIndex)
    {
        target.AllowDrop = true;
        target.DragOver += (_, e) =>
        {
            if (!e.DataView.Contains(StandardDataFormats.Text))
            {
                return;
            }
            e.AcceptedOperation = DataPackageOperation.Copy;
            if (e.DragUIOverride is not null)
            {
                e.DragUIOverride.Caption = Loc.Chrome("queue.drop_insert_caption", "position", computeIndex(e) + 1);
            }
            e.Handled = true;
        };
        target.Drop += async (_, e) =>
        {
            if (!e.DataView.Contains(StandardDataFormats.Text))
            {
                return;
            }
            e.Handled = true;
            var index = computeIndex(e);
            var ids = await ReadDropPayload(e);
            if (ids is null)
            {
                return;
            }
            await ResolveAndApply(ids, (handle, trackIds) =>
            {
                NativeBae.InsertInQueue(handle, trackIds, index);
                return null;
            });
        };
    }

    // Append a card dropped on the queue button to the end of the manual lane.
    // Shares the payload read and resolve path with the in-pane drops; the queue
    // button works whether or not the pane is open.
    public async Task HandleButtonAppendDrop(DragEventArgs e)
    {
        var ids = await ReadDropPayload(e);
        if (ids is null)
        {
            return;
        }
        await ResolveAndApply(ids, (handle, trackIds) => NativeBae.AddToQueue(handle, trackIds));
    }

    // The album grid's bulk Add to Queue / Play Next: resolves album ids to
    // tracks and applies them, sharing the drag-drop resolve-then-apply route
    // and its error banner (the same route macOS's QueueActions gives both the
    // now-playing-bar drop and the grid's bulk menu).
    public Task AddAlbumsToQueue(IReadOnlyList<string> albumIds, bool addNext) =>
        ResolveAndApply(albumIds, (handle, trackIds) => addNext
            ? NativeBae.AddNext(handle, trackIds)
            : NativeBae.AddToQueue(handle, trackIds));

    // Read and decode the drag payload, releasing the drop as soon as the data is
    // read. Null when the payload carries no ids (a Text drag that isn't ours).
    private static async Task<IReadOnlyList<string>?> ReadDropPayload(DragEventArgs e)
    {
        var deferral = e.GetDeferral();
        string payload;
        try
        {
            payload = await e.DataView.GetTextAsync();
        }
        finally
        {
            deferral.Complete();
        }

        var ids = QueueDragPayload.Decode(payload);
        if (ids.Count == 0)
        {
            BaeDiagnostics.Logger.Warning("Ignoring queue drop with no ids in its payload.");
            return null;
        }
        return ids;
    }

    // The manual-lane index a drop at the pointer inserts before: the position of
    // the first realized manual row whose midpoint is below the pointer, else the
    // lane's end. Realized indices are resolved against the flat row collection
    // (offset by the leading Clear + header rows) and translated back to a
    // manual-lane-relative index.
    private static int ComputeInsertIndex(
        ListView list, QueuePaneRowCollection rows, IReadOnlyList<BridgeQueueEntry> manual, DragEventArgs e)
    {
        var pointerY = e.GetPosition(list).Y;
        var realizedRows = new List<RealizedRow>();
        var manualIndex = 0;
        for (var flatIndex = 0; flatIndex < rows.Count; flatIndex++)
        {
            if (rows[flatIndex] is not EntryRow { Lane: QueueLane.Manual })
            {
                continue;
            }
            if (list.ContainerFromIndex(flatIndex) is FrameworkElement container)
            {
                var top = container.TransformToVisual(list).TransformPoint(new Point(0, 0)).Y;
                realizedRows.Add(new RealizedRow(manualIndex, top + container.ActualHeight / 2));
            }
            manualIndex++;
        }
        return QueueDropIndex.Insert(realizedRows, pointerY, manual.Count);
    }

    // Resolve the dragged album/track ids to track ids and hand them to apply
    // (insert at an index, or append), which returns an error message or null.
    // Resolve and apply run off the UI thread; a resolve failure surfaces in the
    // pane's error banner, and an empty resolve is logged and dropped (the core
    // clamps the index, so a queue mutation racing the drop degrades to a clamped
    // insert).
    private async Task ResolveAndApply(IReadOnlyList<string> ids, Func<AppHandle, IReadOnlyList<string>, string?> apply)
    {
        var outcome = await Task.Run(() =>
        {
            var (current, resolved) = _session.WithCurrentHandle(handle => NativeBae.ResolveToTrackIds(handle, ids));
            if (!current)
            {
                return (Current: false, Error: (string?)null, Empty: false);
            }
            var (trackIds, error) = resolved;
            if (error is not null)
            {
                return (Current: true, Error: error, Empty: false);
            }
            if (trackIds is null || trackIds.Count == 0)
            {
                return (Current: true, Error: (string?)null, Empty: true);
            }
            var applyError = _session.WithCurrentHandle(handle => apply(handle, trackIds));
            return (Current: true, Error: applyError.Result, Empty: false);
        });

        if (!outcome.Current)
        {
            return;
        }
        if (outcome.Error is not null)
        {
            _onError(outcome.Error);
            return;
        }
        if (outcome.Empty)
        {
            BaeDiagnostics.Logger.Warning($"Queue drop resolved no tracks for ids [{string.Join(", ", ids)}].");
        }
    }

    // The flat row collection backing the pane's single ListView. Paging is
    // append-only (WinUI's incremental-loading contract only ever asks to load
    // more at the END of the bound collection), which lines up with the context
    // section always being the tail-most section: "near the end of the whole
    // list" is exactly "near the end of the context tail." A reply is merged only
    // if its revision still matches the revision this instance was built for;
    // Rebuild always constructs a fresh instance on every QueueChanged, so a
    // stale in-flight reply lands on an abandoned collection nobody renders.
    private sealed class QueuePaneRowCollection : ObservableCollection<QueueRow>, ISupportIncrementalLoading
    {
        private readonly SessionStore _session;
        private readonly ulong _revision;
        private readonly ulong _contextTotal;
        private ulong _contextLoadedCount;

        public QueuePaneRowCollection(SessionStore session, ulong revision, ulong contextTotal)
        {
            _session = session;
            _revision = revision;
            _contextTotal = contextTotal;
        }

        // Called once from Rebuild after seeding the initial window, so
        // HasMoreItems reflects what's already loaded before any paging.
        public void MarkInitialContextLoad(ulong loadedCount) => _contextLoadedCount = loadedCount;

        public bool HasMoreItems => _contextLoadedCount < _contextTotal;

        public IAsyncOperation<LoadMoreItemsResult> LoadMoreItemsAsync(uint count) =>
            LoadMoreItemsAsyncCore(count).AsAsyncOperation();

        private async Task<LoadMoreItemsResult> LoadMoreItemsAsyncCore(uint count)
        {
            var offset = _contextLoadedCount;
            var (current, resolved) = await _session.RunForCurrentHandle(
                handle => NativeBae.QueueUpcomingPage(handle, checked((uint)offset), count));
            if (!current)
            {
                return new LoadMoreItemsResult { Count = 0 };
            }

            var (page, error) = resolved;
            if (error is not null)
            {
                BaeDiagnostics.Logger.Warning($"Failed to load upcoming queue page at offset {offset}: {error}");
                return new LoadMoreItemsResult { Count = 0 };
            }
            if (page is null || page.Revision != _revision)
            {
                BaeDiagnostics.Logger.Warning(
                    $"Dropping upcoming queue page at offset {offset}: fetched under a since-superseded revision.");
                return new LoadMoreItemsResult { Count = 0 };
            }

            foreach (var entry in page.Entries)
            {
                Add(new EntryRow(QueueLane.Context, entry));
            }
            _contextLoadedCount += (ulong)page.Entries.Length;
            return new LoadMoreItemsResult { Count = (uint)page.Entries.Length };
        }
    }
}
