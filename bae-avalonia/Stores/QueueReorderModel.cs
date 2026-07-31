using System.Collections.Generic;

namespace Bae.Desktop;

// The move core forwards to for a queue reorder: the dragged entry, and the entry
// it now sits before (null when it landed last = the lane's end).
public sealed record QueueMove(string MovedEntryId, string? BeforeEntryId);

// The queue-reorder math: after a drag settles an entry at newIndex within a
// lane, the entry lands before whatever now follows it. Pure over the lane's
// entry ids (a genuine track change starts with no position), so it is unit-tested
// apart from the queue list view and the bridge queue types.
public static class QueueReorderModel
{
    public static QueueMove ResolveMove(IReadOnlyList<string> entryIdsAfterMove, int newIndex)
    {
        var movedId = entryIdsAfterMove[newIndex];
        var beforeIndex = newIndex + 1;
        var beforeEntryId = beforeIndex < entryIdsAfterMove.Count
            ? entryIdsAfterMove[beforeIndex]
            : null;
        return new QueueMove(movedId, beforeEntryId);
    }
}
