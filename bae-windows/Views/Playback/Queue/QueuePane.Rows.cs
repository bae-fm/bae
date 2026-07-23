using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Collections.Specialized;
using System.Globalization;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using Microsoft.UI;
using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.UI;

namespace Bae.Windows;

// The pane's row model: the flat, heterogeneous rows its single ListView
// binds to, and the incremental-loading collection that backs them. Nested in
// QueuePane; split out of QueuePane.cs unchanged.
internal sealed partial class QueuePane
{
    // -- Row model ---------------------------------------------------------
    //
    // The pane's single ListView is bound to one flat, heterogeneous row
    // collection. Every row carries the lane it belongs to; the collection's
    // rows are always laid out as contiguous same-lane runs in `Lane` ordinal
    // order (Manual, then Context) — reorder validation leans on that invariant
    // instead of tracking section boundaries separately.
    private enum QueueLane
    {
        Manual,
        Context,
    }

    private abstract record QueueRow(QueueLane Lane);

    // A section header. Shuffled is non-null only for the context section (the
    // manual lane is never shuffled and has no toggle).
    private sealed record SectionHeaderRow(QueueLane Lane, string Text, bool? Shuffled) : QueueRow(Lane);

    // One queue row's display: the entry and its lane.
    private sealed record EntryRow(QueueLane Lane, BridgeQueueEntry Entry) : QueueRow(Lane)
    {
        internal string EntryId => Entry.EntryId;
    }

    // The empty manual-lane state: shown instead of any manual EntryRow when the
    // lane is empty, so a drop always has a target at the front of the pane.
    private sealed record EmptyManualRow() : QueueRow(QueueLane.Manual);

    // A trailing drop strip appended after a non-empty manual lane, mirroring the
    // reference platforms' "drop here to append" zone. Belongs to Manual — it
    // sits inside that lane's contiguous run, immediately before the context
    // section (or the end of the list).
    private sealed record TrailingDropRow() : QueueRow(QueueLane.Manual);

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
