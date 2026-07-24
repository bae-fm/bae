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

// External album-card drops and the bulk add-to-queue path: payload read,
// insert-index resolution, and the shared resolve-then-apply route. Split out
// of QueuePane.cs unchanged.
internal sealed partial class QueuePane
{
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
            await ResolveAndApply(ids, trackIds => (_queueService.InsertInQueue(trackIds, index), null));
        };
    }

    // Append a card dropped on the queue button to the end of the manual lane.
    // Shares the payload read and resolve path with the in-pane drops; the queue
    // button works whether or not the pane is open.
    private async Task HandleButtonAppendDrop(DragEventArgs e)
    {
        var ids = await ReadDropPayload(e);
        if (ids is null)
        {
            return;
        }
        await ResolveAndApply(ids, trackIds => _queueService.AddToQueue(trackIds));
    }

    // The album grid's bulk Add to Queue / Play Next: resolves album ids to
    // tracks and applies them, sharing the drag-drop resolve-then-apply route
    // and its error banner (the same route macOS's QueueActions gives both the
    // now-playing-bar drop and the grid's bulk menu).
    public Task AddAlbumsToQueue(IReadOnlyList<string> albumIds, bool addNext) =>
        ResolveAndApply(albumIds, trackIds => addNext
            ? _queueService.AddNext(trackIds)
            : _queueService.AddToQueue(trackIds));

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
    // lane's end. Realized indices are resolved against the flat row collection and
    // translated back to a manual-lane-relative index.
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
    private async Task ResolveAndApply(
        IReadOnlyList<string> ids, Func<IReadOnlyList<string>, (bool Current, string? Error)> apply)
    {
        var outcome = await Task.Run(() =>
        {
            var (current, resolved) = _library.ResolveToTrackIds(ids);
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
            var (_, applyError) = apply(trackIds);
            return (Current: true, Error: applyError, Empty: false);
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
