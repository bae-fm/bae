using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Queue mutations — appending, inserting, reordering, removing, jumping to an
/// entry, the shuffle flip, and the upcoming-tail page. The C# mirror of BaeKit's
/// <c>Queue</c> closure-struct. Mutations carry the session-swap currency the
/// Windows session exposes (whether a handle was current, or the error line for
/// the appends that surface one). Every delegate defaults to a fail-loud stub; <see cref="FromSession"/>
/// is the production wiring.
/// </summary>
internal sealed class QueueService
{
    public Func<IReadOnlyList<string>, (bool Current, string? Error)> AddToQueue { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: AddToQueue not wired");

    public Func<IReadOnlyList<string>, (bool Current, string? Error)> AddNext { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: AddNext not wired");

    public Func<string, bool> AddReleaseToQueue { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: AddReleaseToQueue not wired");

    public Func<string, bool> AddReleaseNext { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: AddReleaseNext not wired");

    /// <summary>Insert tracks into the manual lane at an index (core clamps).</summary>
    public Func<IReadOnlyList<string>, int, bool> InsertInQueue { get; init; }
        = (_, _) => throw new InvalidOperationException("QueueService stub: InsertInQueue not wired");

    public Func<string, bool> RemoveEntry { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: RemoveEntry not wired");

    /// <summary>Empty the manual lane, leaving the context lane playing.</summary>
    public Func<bool> ClearUpNext { get; init; }
        = () => throw new InvalidOperationException("QueueService stub: ClearUpNext not wired");

    /// <summary>Drop the context lane; the playing track keeps playing.</summary>
    public Func<bool> ClearPlayingFrom { get; init; }
        = () => throw new InvalidOperationException("QueueService stub: ClearPlayingFrom not wired");

    /// <summary>Move an entry to sit before another; a null target moves it to the
    /// end.</summary>
    public Func<string, string?, bool> ReorderEntry { get; init; }
        = (_, _) => throw new InvalidOperationException("QueueService stub: ReorderEntry not wired");

    public Func<string, bool> SkipToEntry { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: SkipToEntry not wired");

    /// <summary>Flip the playing context between sequential and shuffled order.</summary>
    public Func<bool, bool> SetShuffle { get; init; }
        = _ => throw new InvalidOperationException("QueueService stub: SetShuffle not wired");

    /// <summary>Subscribe to one page of the context's upcoming tail past the
    /// window the queue value already carries.</summary>
    public Func<uint, uint, Action<BridgeQueueUpcomingPage>, Action<Exception>, IDisposable?> SubscribeUpcomingPage { get; init; }
        = (_, _, _, _) => throw new InvalidOperationException("QueueService stub: SubscribeUpcomingPage not wired");

    /// <summary>Wire every mutation through the open session's current handle.</summary>
    public static QueueService FromSession(SessionStore session) => new()
    {
        AddToQueue = trackIds => session.WithCurrentHandle(handle => NativeBae.AddToQueue(handle, trackIds)),
        AddNext = trackIds => session.WithCurrentHandle(handle => NativeBae.AddNext(handle, trackIds)),
        AddReleaseToQueue = releaseId =>
            session.WithCurrentHandle(handle => NativeBae.AddReleaseToQueue(handle, releaseId)),
        AddReleaseNext = releaseId =>
            session.WithCurrentHandle(handle => NativeBae.AddReleaseNext(handle, releaseId)),
        InsertInQueue = (trackIds, index) =>
            session.WithCurrentHandle(handle => NativeBae.InsertInQueue(handle, trackIds, index)),
        RemoveEntry = entryId => session.WithCurrentHandle(handle => NativeBae.QueueRemove(handle, entryId)),
        ClearUpNext = () => session.WithCurrentHandle(NativeBae.QueueClearUpNext),
        ClearPlayingFrom = () => session.WithCurrentHandle(NativeBae.QueueClearPlayingFrom),
        ReorderEntry = (entryId, beforeEntryId) =>
            session.WithCurrentHandle(handle => NativeBae.QueueReorder(handle, entryId, beforeEntryId)),
        SkipToEntry = entryId => session.WithCurrentHandle(handle => NativeBae.QueueSkipTo(handle, entryId)),
        SetShuffle = on => session.WithCurrentHandle(handle => NativeBae.SetShuffle(handle, on)),
        SubscribeUpcomingPage = (offset, limit, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeQueueUpcomingPage(handle, offset, limit, onValue, onError));
            return current ? subscription : null;
        },
    };
}
