using System.Collections.Generic;
using System.Linq;

namespace Bae.Windows;

// The drag payload carried when album cards are dragged into the queue: a
// newline-joined list of ids. Ids are UUIDs, which never contain a newline, so
// the newline is an unambiguous separator and the encoding round-trips exactly.
// One id or many: a single card encodes to its bare id, multiple to the joined
// form, and decode omits empty segments so a trailing or doubled separator can't
// yield a blank id.
public static class QueueDragPayload
{
    public static string Encode(IReadOnlyList<string> ids) => string.Join("\n", ids);

    public static IReadOnlyList<string> Decode(string payload) =>
        payload.Split('\n').Where(segment => segment.Length > 0).ToList();
}

// One queue row the view has realized (rendered a container for): its lane index
// and the vertical midpoint of its container in lane-list coordinates.
// Virtualized rows have no container, so they never appear here.
public readonly record struct RealizedRow(int Index, double MidY);

// The insertion index for a drop over the manual lane: the pointer lands before
// the first row whose midpoint sits below it, else at the end (append). Pure over
// the realized rows the view supplies, so it is unit-tested apart from the WinUI
// list.
public static class QueueDropIndex
{
    // The lane index a drop at pointerY inserts before. With no realized rows
    // (an empty lane, or all rows virtualized out) the drop lands at the front;
    // otherwise it lands before the first realized row whose midpoint is below
    // the pointer, and past the last row (itemCount) when the pointer is below
    // every midpoint.
    public static int Insert(IReadOnlyList<RealizedRow> realizedRowsInOrder, double pointerY, int itemCount)
    {
        foreach (var row in realizedRowsInOrder)
        {
            if (row.MidY > pointerY)
            {
                return row.Index;
            }
        }

        return realizedRowsInOrder.Count == 0 ? 0 : itemCount;
    }
}
