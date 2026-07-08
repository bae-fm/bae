using System.Collections.ObjectModel;
using Microsoft.UI.Dispatching;
using uniffi.bae_bridge;

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

// Mirror of the library grid: the albums and composers on screen, plus the
// per-mode sort selections. The window binds x:Bind Albums/Composers to these
// collections (through forwarders) and drives loads on sort / mode / search
// changes; the store fetches from the current handle, attaches covers, and
// populates the collections, returning an outcome the window renders.
internal sealed class LibraryBrowserStore
{
    private const ulong FirstPageSize = 500;

    private readonly SessionStore _session;
    private readonly DispatcherQueue _dispatcher;

    public SortState Sort { get; } = new();

    public ObservableCollection<Album> Albums { get; } = new();
    public ObservableCollection<ComposerSummary> Composers { get; } = new();

    public LibraryBrowserStore(SessionStore session, DispatcherQueue dispatcher)
    {
        _session = session;
        _dispatcher = dispatcher;
    }

    // Load the album grid (newest first, per the active album sort) from the
    // current handle into Albums.
    public BrowserGridLoad LoadAlbums()
    {
        var sort = Sort.Active;
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.AlbumPage(handle, 0, FirstPageSize, sort.Field, sort.Ascending));
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

        return new BrowserGridLoad(BrowserMode.Albums, BrowserLoadResult.Loaded, null, page.Albums.Count == 0);
    }

    // Load the composer grid (per the active composer sort) into Composers. Clears
    // the grid up front so a failed or empty load leaves nothing stale behind.
    public BrowserGridLoad LoadComposers()
    {
        Composers.Clear();
        var sort = Sort.Active;
        var (current, page) = _session.WithCurrentHandle(
            handle => NativeBae.ComposerPage(handle, 0, FirstPageSize, sort.Field, sort.Ascending));
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

        return new BrowserGridLoad(BrowserMode.Composers, BrowserLoadResult.Loaded, null, page.Composers.Count == 0);
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
    }
}
