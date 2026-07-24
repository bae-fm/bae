using System;
using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// Library reads — album/composer/artist pagination, their totals, and search.
/// The C# mirror of BaeKit's <c>Library</c> closure-struct: one stored delegate
/// per read, each wired through <see cref="SessionStore.WithCurrentHandle{T}"/>
/// so it carries the session-swap currency contract (the <c>Current</c> flag)
/// the browser store already branches on. Views and stores take this narrow
/// service instead of reaching for the session and <see cref="NativeBae"/>.
/// Every delegate defaults to a fail-loud stub; <see cref="FromSession"/> is the
/// production wiring.
/// </summary>
internal sealed class LibraryService
{
    public Func<ulong, ulong, IReadOnlyList<SortCriterion<AlbumSortField>>, (bool Current, (List<Album>? Albums, string? Error) Page)> AlbumPage { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: AlbumPage not wired");

    public Func<(bool Current, long Count)> AlbumCount { get; init; }
        = () => throw new InvalidOperationException("LibraryService stub: AlbumCount not wired");

    public Func<ulong, ulong, IReadOnlyList<SortCriterion<ComposerSortField>>, (bool Current, (List<ComposerSummary>? Composers, string? Error) Page)> ComposerPage { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: ComposerPage not wired");

    public Func<(bool Current, long Count)> ComposerCount { get; init; }
        = () => throw new InvalidOperationException("LibraryService stub: ComposerCount not wired");

    public Func<ulong, ulong, IReadOnlyList<SortCriterion<ArtistSortField>>, (bool Current, (List<ArtistSummary>? Artists, string? Error) Page)> ArtistPage { get; init; }
        = (_, _, _) => throw new InvalidOperationException("LibraryService stub: ArtistPage not wired");

    public Func<(bool Current, long Count)> ArtistCount { get; init; }
        = () => throw new InvalidOperationException("LibraryService stub: ArtistCount not wired");

    public Func<string, (bool Current, (LibrarySearchResults? Results, string? Error) Result)> Search { get; init; }
        = _ => throw new InvalidOperationException("LibraryService stub: Search not wired");

    /// <summary>Wire every read through the open session's current handle.</summary>
    public static LibraryService FromSession(SessionStore session) => new()
    {
        AlbumPage = (offset, limit, criteria) =>
            session.WithCurrentHandle(handle => NativeBae.AlbumPage(handle, offset, limit, criteria)),
        AlbumCount = () => session.WithCurrentHandle(NativeBae.AlbumCount),
        ComposerPage = (offset, limit, criteria) =>
            session.WithCurrentHandle(handle => NativeBae.ComposerPage(handle, offset, limit, criteria)),
        ComposerCount = () => session.WithCurrentHandle(NativeBae.ComposerCount),
        ArtistPage = (offset, limit, criteria) =>
            session.WithCurrentHandle(handle => NativeBae.ArtistPage(handle, offset, limit, criteria)),
        ArtistCount = () => session.WithCurrentHandle(NativeBae.ArtistCount),
        Search = query => session.WithCurrentHandle(handle => NativeBae.Search(handle, query)),
    };
}
