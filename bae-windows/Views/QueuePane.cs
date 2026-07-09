using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;

namespace Bae.Windows;

// The queue side pane: a non-modal surface hosting the manual "Up Next" lane and
// the playback context as two reorderable lists. Non-modal so the album grid
// behind it can source a drag while the pane stays open — album cards drop into
// the manual lane at a chosen index. Reads the lanes from the playback store and
// rebuilds on its QueueChanged while visible; each row skips on click, removes on
// right-tap, and reorders on drag (forwarded to core by entry id). Clear empties
// only the manual lane. External drops land only in the manual lane; the context
// lane rejects them.
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

    // One queue row's display: the entry, its id, and the one-line summary.
    private sealed record QueueEntryRow(BridgeQueueEntry Entry)
    {
        internal string EntryId => Entry.EntryId;
        public override string ToString() =>
            $"{Entry.Title} — {Entry.ArtistNames} · {Loc.Duration(Entry.DurationMs)}".Trim();
    }

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
    // opening).
    private void Rebuild()
    {
        var body = new StackPanel { Spacing = 8 };

        // Clear empties only the manual lane (the context survives), so it
        // disables on an empty manual lane regardless of the context.
        var clear = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            IsEnabled = _playback.ManualQueue.Count > 0,
        };
        clear.Click += (_, _) => _session.WithCurrentHandle(NativeBae.QueueClear);
        body.Children.Add(clear);

        // The manual lane ("Up Next"), always shown so its empty state can accept
        // a drop at the front; then the playback context (the release being played
        // from) as its own reorderable list.
        body.Children.Add(QueueSectionLabel(Loc.Chrome("queue.section.up_next")));
        AddManualLane(body, _playback.ManualQueue);

        if (_playback.Context is { Upcoming.Length: > 0 } ctx)
        {
            // The context section names what's playing — a release ("Playing From")
            // vs the whole library — by the source kind the wire shape carries.
            var labelKey = ctx.Kind == BridgePlaybackSourceKind.Library
                ? "queue.section.your_library"
                : "queue.section.playing_from";
            body.Children.Add(ContextSectionLabel(Loc.Chrome(labelKey), ctx.Shuffled));
            body.Children.Add(BuildQueueLaneList(ctx.Upcoming));
        }

        var content = new Grid();
        content.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        content.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });

        var header = BuildHeader();
        Grid.SetRow(header, 0);
        content.Children.Add(header);

        var scroll = new ScrollViewer
        {
            Content = body,
            Padding = new Thickness(16, 8, 16, 16),
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };
        Grid.SetRow(scroll, 1);
        content.Children.Add(scroll);

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
            Content = new FontIcon { Glyph = "", FontSize = 14 },
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

    // Add the manual lane to the body: the reorderable list plus a trailing
    // append zone when it has entries, or an empty-state drop area (index 0) when
    // it is empty. Both accept external album drops.
    private void AddManualLane(Panel body, IReadOnlyList<BridgeQueueEntry> manual)
    {
        if (manual.Count == 0)
        {
            body.Children.Add(BuildEmptyDropArea());
            return;
        }

        var laneList = BuildQueueLaneList(manual);
        AttachExternalDrop(laneList, e => ComputeInsertIndex(laneList, manual, e));
        body.Children.Add(laneList);

        // A drop strip below the list appends at the lane's end, mirroring the
        // trailing zone on the reference platform.
        var trailing = new Border
        {
            Height = 40,
            Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Transparent),
        };
        AttachExternalDrop(trailing, _ => manual.Count);
        body.Children.Add(trailing);
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
        var area = new Border
        {
            MinHeight = 96,
            Padding = new Thickness(16),
            CornerRadius = new CornerRadius(6),
            Background = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["CardBackgroundFillColorDefaultBrush"],
            Child = stack,
        };
        AttachExternalDrop(area, _ => 0);
        return area;
    }

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
                Glyph = "",
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

    // One lane's reorderable list: click skips, right-tap removes, drag reorders
    // within the lane (the framework raises a Move, forwarded to core by entry id).
    private ListView BuildQueueLaneList(IEnumerable<BridgeQueueEntry> items)
    {
        var queueItems = new ObservableCollection<QueueEntryRow>(items.Select(item => new QueueEntryRow(item)));
        queueItems.CollectionChanged += (_, args) =>
        {
            if (args.Action == System.Collections.Specialized.NotifyCollectionChangedAction.Move)
            {
                // The collection already reflects the move: resolve the entry and
                // the id it now sits before (null when it's now last).
                var move = QueueReorderModel.ResolveMove(
                    queueItems.Select(row => row.EntryId).ToList(), args.NewStartingIndex);
                _session.WithCurrentHandle(
                    handle => NativeBae.QueueReorder(handle, move.MovedEntryId, move.BeforeEntryId));
            }
        };

        var list = new ListView
        {
            ItemsSource = queueItems,
            SelectionMode = ListViewSelectionMode.None,
            IsItemClickEnabled = true,
            CanReorderItems = true,
            CanDragItems = true,
            AllowDrop = true,
        };
        list.ItemClick += (_, args) =>
        {
            if (args.ClickedItem is QueueEntryRow clicked)
            {
                _session.WithCurrentHandle(handle => NativeBae.QueueSkipTo(handle, clicked.EntryId));
            }
        };
        // Right-tap a row to drop it from the queue. Removing locally too keeps the
        // pane in sync; the Move-only reorder handler ignores this Remove.
        list.RightTapped += (_, args) =>
        {
            if (args.OriginalSource is not FrameworkElement element
                || element.DataContext is not QueueEntryRow item)
            {
                return;
            }

            var index = queueItems.IndexOf(item);
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
                var idx = queueItems.IndexOf(item);
                if (idx < 0)
                {
                    return;
                }
                _session.WithCurrentHandle(handle => NativeBae.QueueRemove(handle, item.EntryId));
                queueItems.RemoveAt(idx);
            };
            menu.Items.Add(remove);
            menu.ShowAt(element, new FlyoutShowOptions { Position = args.GetPosition(element) });
        };

        return list;
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
    // the first realized row whose midpoint is below the pointer, else the end.
    private static int ComputeInsertIndex(ListView laneList, IReadOnlyList<BridgeQueueEntry> manual, DragEventArgs e)
    {
        var pointerY = e.GetPosition(laneList).Y;
        var rows = new List<RealizedRow>();
        for (var i = 0; i < manual.Count; i++)
        {
            // Virtualized rows have no container; skip them and let the model
            // interpolate from the realized ones.
            if (laneList.ContainerFromIndex(i) is FrameworkElement container)
            {
                var top = container.TransformToVisual(laneList).TransformPoint(new Point(0, 0)).Y;
                rows.Add(new RealizedRow(i, top + container.ActualHeight / 2));
            }
        }
        return QueueDropIndex.Insert(rows, pointerY, manual.Count);
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
}
