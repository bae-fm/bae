using System.Collections.ObjectModel;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The queue dialog: the manual "Up Next" lane and the playback context as two
// reorderable lists. Reads the lanes from the playback store; each row skips on
// click, removes on right-tap, and reorders on drag (forwarded to core by entry
// id). Clear empties only the manual lane.
internal sealed class QueueDialog
{
    private readonly SessionStore _session;
    private readonly PlaybackStore _playback;
    private readonly System.Func<XamlRoot?> _xamlRoot;

    public QueueDialog(SessionStore session, PlaybackStore playback, System.Func<XamlRoot?> xamlRoot)
    {
        _session = session;
        _playback = playback;
        _xamlRoot = xamlRoot;
    }

    // One queue row's display: the entry, its id, and the one-line summary.
    private sealed record QueueEntryRow(BridgeQueueEntry Entry)
    {
        internal string EntryId => Entry.EntryId;
        public override string ToString() =>
            $"{Entry.Title} — {Entry.ArtistNames} · {Loc.Duration(Entry.DurationMs)}".Trim();
    }

    public async System.Threading.Tasks.Task Show()
    {
        if (_session.CurrentHandleOrNull() == null)
        {
            return;
        }

        // Clear empties only the manual lane (the context survives), so it
        // disables on an empty manual lane regardless of the context.
        var clear = new Button
        {
            Content = Loc.Chrome("queue.clear"),
            IsEnabled = _playback.ManualQueue.Count > 0,
        };
        clear.Click += (_, _) => _session.WithCurrentHandle(NativeBae.QueueClear);

        var content = new StackPanel { Spacing = 8 };
        content.Children.Add(clear);

        // Two distinct sections: the manual lane ("Up Next"), then the context
        // (the release being played from). Each is its own reorderable list —
        // entry ids are unique across both and core no-ops a cross-lane move.
        if (_playback.ManualQueue.Count > 0)
        {
            content.Children.Add(QueueSectionLabel(Loc.Chrome("queue.section.up_next")));
            content.Children.Add(BuildQueueLaneList(_playback.ManualQueue));
        }

        if (_playback.Context is { Upcoming.Length: > 0 } ctx)
        {
            // The context section names what's playing — a release ("Playing From")
            // vs the whole library — by the source kind the wire shape carries.
            var labelKey = ctx.Kind == BridgePlaybackSourceKind.Library
                ? "queue.section.your_library"
                : "queue.section.playing_from";
            content.Children.Add(ContextSectionLabel(Loc.Chrome(labelKey), ctx.Shuffled));
            content.Children.Add(BuildQueueLaneList(ctx.Upcoming));
        }

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("queue.title"),
            Content = content,
            CloseButtonText = Loc.Chrome("action.close"),
            XamlRoot = _xamlRoot(),
        };
        await dialog.ShowAsync();
    }

    // A plain section header for the queue dialog (the manual "Up Next" lane,
    // which is never shuffled and has no shuffle control).
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
                Glyph = "\uE8B1",
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
    private ListView BuildQueueLaneList(System.Collections.Generic.IEnumerable<BridgeQueueEntry> items)
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
        // open dialog in sync; the Move-only reorder handler ignores this Remove.
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
                // the open dialog's collection in sync.
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
}
