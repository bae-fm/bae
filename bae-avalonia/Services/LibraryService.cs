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
    public Func<ulong, ulong, IReadOnlyList<SortCriterion<AlbumSortField>>, (bool Current, (List<Album>? Albums, string? Error) Page)> AlbumPage { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: AlbumPage not wired");

    public Func<(bool Current, long Count)> AlbumCount { get; init; }
        = () => throw new InvalidOperationException("LibraryService stub: AlbumCount not wired");

    /// <summary>One album's full detail — every release with its tracks — for the
    /// inline album expansion. Async: the Windows consumer opened it off the UI
    /// thread. The C# mirror of BaeKit's <c>Library.getAlbumDetail</c>.</summary>
    public Func<string, Task<(bool Current, (AlbumDetail? Detail, string? Error) Response)>> AlbumDetail { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: AlbumDetail not wired");

    /// <summary>The 0-based position of an album under the active sort, or null
    /// when it isn't present — lets a reveal page in and scroll to it.</summary>
    public Func<IReadOnlyList<SortCriterion<AlbumSortField>>, string, Task<(bool Current, (long? Index, string? Error) Result)>> AlbumIndex { get; init; }
        = (_, _) => throw new InvalidOperationException("LibraryService stub: AlbumIndex not wired");

    public Func<ulong, ulong, IReadOnlyList<SortCriterion<ComposerSortField>>, (bool Current, (List<ComposerSummary>? Composers, string? Error) Page)> ComposerPage { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: ComposerPage not wired");

    public Func<(bool Current, long Count)> ComposerCount { get; init; }
        = () => throw new InvalidOperationException("LibraryService stub: ComposerCount not wired");

    public Func<string, Task<(bool Current, (ComposerDetail? Detail, string? Error) Response)>> ComposerDetail { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: ComposerDetail not wired");

    public Func<string, Task<(bool Current, (WorkDetail? Detail, string? Error) Response)>> WorkDetail { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: WorkDetail not wired");

    public Func<ulong, ulong, IReadOnlyList<SortCriterion<ArtistSortField>>, (bool Current, (List<ArtistSummary>? Artists, string? Error) Page)> ArtistPage { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: ArtistPage not wired");

    public Func<(bool Current, long Count)> ArtistCount { get; init; }
        = () => throw new InvalidOperationException("LibraryService stub: ArtistCount not wired");

    public Func<string, Task<(bool Current, (ArtistDetail? Detail, string? Error) Response)>> ArtistDetail { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: ArtistDetail not wired");

    public Func<string, (bool Current, (LibrarySearchResults? Results, string? Error) Result)> Search { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: Search not wired");

    /// <summary>Total releases matching a storage tab, for the storage dialog's
    /// incremental collection to know when it has everything.</summary>
    public Func<StorageTab, Task<(bool Current, long Count)>> StorageCount { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: StorageCount not wired");

    /// <summary>Sum of file sizes over every release matching a storage tab — the
    /// dialog footer's total, independent of how many pages have loaded.</summary>
    public Func<StorageTab, Task<(bool Current, long Bytes)>> StorageTotalSize { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: StorageTotalSize not wired");

    /// <summary>One page of storage rows for a tab, sorted server-side. Synchronous
    /// to back the incremental collection's paging callback.</summary>
    public Func<StorageTab, StorageSortField, SortDirection, ulong, ulong, (bool Current, (BridgeStoragePage? Page, string? Error) Result)> StoragePage { get; init; }
        = (_, _, _, _, _) => throw new InvalidOperationException("LibraryService stub: StoragePage not wired");

    /// <summary>A single release's live storage fields (state, pin, actions,
    /// in-flight transfer), for refreshing the album-detail storage band. Null when
    /// the release is gone.</summary>
    public Func<string, Task<(bool Current, (Release? Release, string? Error) Result)>> ReleaseStorage { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: ReleaseStorage not wired");

    /// <summary>Prefetch a release's metadata from a source for the import edit-form
    /// seed — the raw projection before the confirm dialog composes it.</summary>
    public Func<string, BridgeMetadataSource, uint?, Task<(bool Current, (BridgeReleasePrefetch? Prefetch, string? Error) Result)>> PrefetchRelease { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: PrefetchRelease not wired");

    /// <summary>Expand album/track ids to flat track-id lists (album ids resolve to
    /// their primary release's tracks), for a queue insert carrying a drag
    /// payload.</summary>
    public Func<IReadOnlyList<string>, (bool Current, (List<string>? Value, string? Error) Result)> ResolveToTrackIds { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: ResolveToTrackIds not wired");

    /// <summary>Whether the library page spans the window's full width instead of a
    /// width-capped column. The write's config invalidation re-renders the page.</summary>
    public Func<bool, (bool Current, string? Error)> SetLibraryFullWidth { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: SetLibraryFullWidth not wired");

    /// <summary>Wire every read through the open session's current handle.</summary>
    public static LibraryService FromSession(SessionStore session) => new()
    {
        AlbumPage = (offset, limit, criteria) =>
            session.WithCurrentHandle(handle => NativeBae.AlbumPage(handle, offset, limit, criteria)),
        AlbumCount = () => session.WithCurrentHandle(NativeBae.AlbumCount),
        AlbumDetail = albumId =>
            session.RunForCurrentHandle(handle => NativeBae.GetAlbumDetail(handle, albumId)),
        AlbumIndex = (criteria, albumId) =>
            session.RunForCurrentHandle(handle => NativeBae.AlbumIndex(handle, criteria, albumId)),
        ComposerPage = (offset, limit, criteria) =>
            session.WithCurrentHandle(handle => NativeBae.ComposerPage(handle, offset, limit, criteria)),
        ComposerCount = () => session.WithCurrentHandle(NativeBae.ComposerCount),
        ComposerDetail = artistId =>
            session.RunForCurrentHandle(handle => NativeBae.GetComposerDetail(handle, artistId)),
        WorkDetail = workId =>
            session.RunForCurrentHandle(handle => NativeBae.GetWorkDetail(handle, workId)),
        ArtistPage = (offset, limit, criteria) =>
            session.WithCurrentHandle(handle => NativeBae.ArtistPage(handle, offset, limit, criteria)),
        ArtistCount = () => session.WithCurrentHandle(NativeBae.ArtistCount),
        ArtistDetail = artistId =>
            session.RunForCurrentHandle(handle => NativeBae.GetArtistDetail(handle, artistId)),
        Search = query => session.WithCurrentHandle(handle => NativeBae.Search(handle, query)),
        StorageCount = tab => session.RunForCurrentHandle(handle => NativeBae.StorageCount(handle, tab)),
        StorageTotalSize = tab => session.RunForCurrentHandle(handle => NativeBae.StorageTotalSize(handle, tab)),
        StoragePage = (tab, field, direction, offset, limit) =>
            session.WithCurrentHandle(handle => NativeBae.StoragePage(handle, tab, field, direction, offset, limit)),
        ReleaseStorage = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.ReleaseStorage(handle, releaseId)),
        PrefetchRelease = (releaseId, source, localTrackCount) =>
            session.RunForCurrentHandle(handle => NativeBae.PrefetchRelease(handle, releaseId, source, localTrackCount)),
        ResolveToTrackIds = ids =>
            session.WithCurrentHandle(handle => NativeBae.ResolveToTrackIds(handle, ids)),
        SetLibraryFullWidth = enabled =>
            session.WithCurrentHandle(handle => NativeBae.SetLibraryFullWidth(handle, enabled)),
    };
}
