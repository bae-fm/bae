using System;
using System.Collections.Generic;
using Microsoft.UI.Dispatching;

namespace Bae.Windows;

// How a browser grid load ended: the session closed mid-load (HandleGone, leave
// the view untouched), the fetch failed (Failed, Error carries the line or null
// for the generic fallback), or the grid loaded (Loaded, IsEmpty picks the mode's
// empty-state message). The window renders pane visibility and the status line
// from this; the store never touches XAML.
internal enum BrowserLoadResult
{
    HandleGone,
    Failed,
    Loaded,
}

internal sealed record BrowserGridLoad(BrowserMode Mode, BrowserLoadResult Result, string? Error, bool IsEmpty);

// The cover-attached search results plus any error, for the search pane to render.
// HandleGone means the session closed mid-search; the window leaves the pane as-is.
internal sealed record BrowserSearch(bool HandleGone, LibrarySearchResults? Results, string? Error);

// Mirror of the library grid: the albums, composers, and artists on screen,
// plus the per-mode sort selections. The window binds x:Bind Albums/Composers to
// these collections (through forwarders) and drives loads on sort / mode /
// search changes; the store fetches from the current handle, attaches covers,
// and populates the collections, returning an outcome the window renders. Each
// collection eagerly loads its first page (so the synchronous BrowserGridLoad
// contract below keeps working) and pages the rest in incrementally as the
// grid/list scrolls.
internal sealed class LibraryBrowserStore
{
    private const ulong PageSize = 200;

    private readonly SessionStore _session;
    private readonly DispatcherQueue _dispatcher;

    public LibrarySort Sort { get; }

    public IncrementalBrowserCollection<Album> Albums { get; }
    public IncrementalBrowserCollection<ComposerSummary> Composers { get; }
    public IncrementalBrowserCollection<ArtistSummary> Artists { get; }

    public LibraryBrowserStore(SessionStore session, DispatcherQueue dispatcher)
    {
        _session = session;
        _dispatcher = dispatcher;
        Albums = new IncrementalBrowserCollection<Album>(FetchAlbumPage, LogPageError("albums"));
        Composers = new IncrementalBrowserCollection<ComposerSummary>(FetchComposerPage, LogPageError("composers"));
        Artists = new IncrementalBrowserCollection<ArtistSummary>(FetchArtistPage, LogPageError("artists"));
        Sort = LibrarySortStore.Load();
        // All three sorts persist like macOS's: write the criteria back on every
        // change.
        Sort.Albums.Changed += () => LibrarySortStore.SaveAlbums(Sort.Albums);
        Sort.Composers.Changed += () => LibrarySortStore.SaveComposers(Sort.Composers);
        Sort.Artists.Changed += () => LibrarySortStore.SaveArtists(Sort.Artists);
    }

    private static Action<string?> LogPageError(string what) => error =>
        BaeDiagnostics.Logger.Warning($"Failed to load a page of {what} past the first: {error}");

    // Load the album grid, ordered by the active album sort criteria: the first
    // page eagerly (for the synchronous status-line contract), then seed the
    // collection so the rest pages in as the grid scrolls.
    public BrowserGridLoad LoadAlbums()
    {
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.AlbumPage(handle, 0, PageSize, Sort.Albums.Items));
        if (!current)
        {
            return new BrowserGridLoad(BrowserMode.Albums, BrowserLoadResult.HandleGone, null, false);
        }

        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return new BrowserGridLoad(BrowserMode.Albums, BrowserLoadResult.HandleGone, null, false);
        }

        Albums.Clear();
        if (page.Error is not null || page.Albums is null)
        {
            return new BrowserGridLoad(BrowserMode.Albums, BrowserLoadResult.Failed, page.Error, false);
        }

        foreach (var album in page.Albums)
        {
            album.AttachCover(handle, _dispatcher);
            Albums.Add(album);
        }
        var (countCurrent, total) = _session.WithCurrentHandle(NativeBae.AlbumCount);
        Albums.SeedFirstPage(countCurrent ? checked((ulong)total) : (ulong)page.Albums.Count, (ulong)page.Albums.Count);

        return new BrowserGridLoad(BrowserMode.Albums, BrowserLoadResult.Loaded, null, page.Albums.Count == 0);
    }

    // Load the composer grid (per the active composer sort): the first page
    // eagerly, then seed incremental paging for the rest. Clears the grid up
    // front so a failed or empty load leaves nothing stale behind.
    public BrowserGridLoad LoadComposers()
    {
        Composers.Clear();
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.ComposerPage(handle, 0, PageSize, Sort.Composers.Items));
        if (!current)
        {
            return new BrowserGridLoad(BrowserMode.Composers, BrowserLoadResult.HandleGone, null, false);
        }
        if (page.Error is not null || page.Composers is null)
        {
            return new BrowserGridLoad(BrowserMode.Composers, BrowserLoadResult.Failed, page.Error, false);
        }

        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return new BrowserGridLoad(BrowserMode.Composers, BrowserLoadResult.HandleGone, null, false);
        }

        foreach (var composer in page.Composers)
        {
            composer.AttachCover(handle, _dispatcher);
            Composers.Add(composer);
        }
        var (countCurrent, total) = _session.WithCurrentHandle(NativeBae.ComposerCount);
        Composers.SeedFirstPage(countCurrent ? checked((ulong)total) : (ulong)page.Composers.Count, (ulong)page.Composers.Count);

        return new BrowserGridLoad(BrowserMode.Composers, BrowserLoadResult.Loaded, null, page.Composers.Count == 0);
    }

    // Load the artist list (per the active artist sort): the first page eagerly,
    // then seed incremental paging for the rest. Clears the list up front so a
    // failed or empty load leaves nothing stale behind.
    public BrowserGridLoad LoadArtists()
    {
        Artists.Clear();
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.ArtistPage(handle, 0, PageSize, Sort.Artists.Items));
        if (!current)
        {
            return new BrowserGridLoad(BrowserMode.Artists, BrowserLoadResult.HandleGone, null, false);
        }
        if (page.Error is not null || page.Artists is null)
        {
            return new BrowserGridLoad(BrowserMode.Artists, BrowserLoadResult.Failed, page.Error, false);
        }

        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return new BrowserGridLoad(BrowserMode.Artists, BrowserLoadResult.HandleGone, null, false);
        }

        foreach (var artist in page.Artists)
        {
            artist.AttachCover(handle, _dispatcher);
            Artists.Add(artist);
        }
        var (countCurrent, total) = _session.WithCurrentHandle(NativeBae.ArtistCount);
        Artists.SeedFirstPage(countCurrent ? checked((ulong)total) : (ulong)page.Artists.Count, (ulong)page.Artists.Count);

        return new BrowserGridLoad(BrowserMode.Artists, BrowserLoadResult.Loaded, null, page.Artists.Count == 0);
    }

    // Fetch one page of albums past the first (offset > 0), attaching covers the
    // same way the eager first page does.
    private (bool Current, IReadOnlyList<Album>? Items, string? Error) FetchAlbumPage(
        ulong offset, ulong limit)
    {
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.AlbumPage(handle, offset, limit, Sort.Albums.Items));
        if (!current)
        {
            return (false, null, null);
        }
        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return (false, null, null);
        }
        if (page.Error is not null || page.Albums is null)
        {
            return (true, null, page.Error);
        }
        foreach (var album in page.Albums)
        {
            album.AttachCover(handle, _dispatcher);
        }
        return (true, page.Albums, null);
    }

    private (bool Current, IReadOnlyList<ComposerSummary>? Items, string? Error) FetchComposerPage(
        ulong offset, ulong limit)
    {
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.ComposerPage(handle, offset, limit, Sort.Composers.Items));
        if (!current)
        {
            return (false, null, null);
        }
        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return (false, null, null);
        }
        if (page.Error is not null || page.Composers is null)
        {
            return (true, null, page.Error);
        }
        foreach (var composer in page.Composers)
        {
            composer.AttachCover(handle, _dispatcher);
        }
        return (true, page.Composers, null);
    }

    private (bool Current, IReadOnlyList<ArtistSummary>? Items, string? Error) FetchArtistPage(
        ulong offset, ulong limit)
    {
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.ArtistPage(handle, offset, limit, Sort.Artists.Items));
        if (!current)
        {
            return (false, null, null);
        }
        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return (false, null, null);
        }
        if (page.Error is not null || page.Artists is null)
        {
            return (true, null, page.Error);
        }
        foreach (var artist in page.Artists)
        {
            artist.AttachCover(handle, _dispatcher);
        }
        return (true, page.Artists, null);
    }

    // Run a library search and attach covers to the results, for the search pane to
    // render. Covers are attached only when the search returned no error; an
    // error result passes through untouched.
    public BrowserSearch Search(string query)
    {
        var (current, search) = _session.WithCurrentHandle(handle => NativeBae.Search(handle, query));
        if (!current)
        {
            return new BrowserSearch(HandleGone: true, null, null);
        }

        var handle = _session.CurrentHandleOrNull();
        if (handle == null)
        {
            return new BrowserSearch(HandleGone: true, null, null);
        }

        var results = search.Results;
        if (search.Error is null && results is not null)
        {
            foreach (var album in results.Albums)
            {
                album.AttachCover(handle, _dispatcher);
            }
            foreach (var track in results.Tracks)
            {
                track.AttachCover(handle, _dispatcher);
            }
            foreach (var composer in results.Composers)
            {
                composer.AttachCover(handle, _dispatcher);
            }
            foreach (var work in results.Works)
            {
                work.AttachCover(handle, _dispatcher);
            }
        }

        return new BrowserSearch(HandleGone: false, results, search.Error);
    }

    // Drop the loaded grids on teardown so the next library doesn't inherit them.
    public void Reset()
    {
        Albums.Clear();
        Composers.Clear();
        Artists.Clear();
    }
}
