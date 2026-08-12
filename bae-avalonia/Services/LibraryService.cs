using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Library reads — album/composer/artist pagination and detail, search, storage
/// listing, release detail, the import metadata prefetch, id resolution, plus the
/// library page's own display-preference write. The C# mirror of BaeKit's
/// <c>Library</c> closure-struct: one stored delegate per read, each wired through
/// the open session so it carries the session-swap currency contract (the
/// <c>Current</c> flag) stores already branch on. Reads whose Windows consumers
/// run them off the UI thread are async (they wrap
/// <see cref="SessionStore.RunForCurrentHandle{T}"/>); the synchronous
/// first-page/paging reads wrap <see cref="SessionStore.WithCurrentHandle{T}"/>.
/// Every delegate defaults to a fail-loud stub; <see cref="FromSession"/> is the
/// production wiring.
/// </summary>
internal sealed class LibraryService
{
    public Func<ulong, ulong, IReadOnlyList<SortCriterion<AlbumSortField>>, Action<IReadOnlyList<Album>, int>, Action<Exception>, IDisposable?> SubscribeAlbumPage { get; init; }
        = (_, _, _, _, _) => throw new InvalidOperationException("LibraryService stub: SubscribeAlbumPage not wired");

    /// <summary>One album's full detail — every release with its tracks — for the
    /// inline album expansion. Async: the Windows consumer opened it off the UI
    /// thread. The C# mirror of BaeKit's <c>Library.getAlbumDetail</c>.</summary>
    public Func<string, Action<AlbumDetail?>, Action<Exception>, IDisposable?> SubscribeAlbumDetail { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: SubscribeAlbumDetail not wired");

    /// <summary>The 0-based position of an album under the active sort, or null
    /// when it isn't present — lets a reveal page in and scroll to it.</summary>
    public Func<IReadOnlyList<SortCriterion<AlbumSortField>>, string, Task<(bool Current, (long? Index, string? Error) Result)>> AlbumIndex { get; init; }
        = (_, _) => throw new InvalidOperationException("LibraryService stub: AlbumIndex not wired");

    public Func<ulong, ulong, IReadOnlyList<SortCriterion<ComposerSortField>>, Action<IReadOnlyList<ComposerSummary>, int>, Action<Exception>, IDisposable?> SubscribeComposerPage { get; init; }
        = (_, _, _, _, _) => throw new InvalidOperationException("LibraryService stub: SubscribeComposerPage not wired");

    public Func<ulong, ulong, IReadOnlyList<SortCriterion<ArtistSortField>>, Action<IReadOnlyList<ArtistSummary>, int>, Action<Exception>, IDisposable?> SubscribeArtistPage { get; init; }
        = (_, _, _, _, _) => throw new InvalidOperationException("LibraryService stub: SubscribeArtistPage not wired");

    /// <summary>Total releases matching a storage tab — the storage dialog's page
    /// source count, so the incremental list knows the row total up front.
    /// Synchronous, run off the UI thread by the page source (like the browse
    /// counts).</summary>
    public Func<StorageTab, StorageSortField, SortDirection, ulong, ulong,
        Action<IReadOnlyList<BridgeStorageRow>, int, long>, Action<Exception>, IDisposable?> SubscribeStorage { get; init; }
        = (_, _, _, _, _, _, _) => throw new InvalidOperationException("LibraryService stub: SubscribeStorage not wired");

    /// <summary>Expand album/track ids to flat track-id lists (album ids resolve to
    /// their primary release's tracks), for a queue insert carrying a drag
    /// payload.</summary>
    public Func<IReadOnlyList<string>, (bool Current, (List<string>? Value, string? Error) Result)> ResolveToTrackIds { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: ResolveToTrackIds not wired");

    /// <summary>Whether the library page spans the window's full width instead of a
    /// width-capped column. The config subscription re-renders the page.</summary>
    public Func<bool, (bool Current, string? Error)> SetLibraryFullWidth { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: SetLibraryFullWidth not wired");

    /// <summary>Wire every read through the open session's current handle.</summary>
    public static LibraryService FromSession(SessionStore session) => new()
    {
        SubscribeAlbumPage = (offset, limit, criteria, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeAlbumPage(handle, offset, limit, criteria, onValue, onError));
            return current ? subscription : null;
        },
        SubscribeAlbumDetail = (albumId, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeAlbumDetail(handle, albumId, onValue, onError));
            return current ? subscription : null;
        },
        AlbumIndex = (criteria, albumId) =>
            session.RunForCurrentHandle(handle => NativeBae.AlbumIndex(handle, criteria, albumId)),
        SubscribeComposerPage = (offset, limit, criteria, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeComposerPage(handle, offset, limit, criteria, onValue, onError));
            return current ? subscription : null;
        },
        SubscribeArtistPage = (offset, limit, criteria, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeArtistPage(handle, offset, limit, criteria, onValue, onError));
            return current ? subscription : null;
        },
        SubscribeStorage = (tab, field, direction, offset, limit, onValue, onError) =>
        {
            var (current, subscription) = session.WithCurrentHandle(handle =>
                NativeBae.SubscribeStorage(handle, tab, field, direction, offset, limit, onValue, onError));
            return current ? subscription : null;
        },
        ResolveToTrackIds = ids =>
            session.WithCurrentHandle(handle => NativeBae.ResolveToTrackIds(handle, ids)),
        SetLibraryFullWidth = enabled =>
            session.WithCurrentHandle(handle => NativeBae.SetLibraryFullWidth(handle, enabled)),
    };
}
