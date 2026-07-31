using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Avalonia.Threading;

namespace Bae.Desktop;

/// <summary>
/// The library browser's data layer, built on <see cref="PaginatedList{TRow, TId}"/>
/// (the incremental-loading core) rather than an append-only collection.
/// It owns one paginated list per browse mode (albums / composers / artists) plus
/// the per-mode sort, and the side stores the lists intern their rows into: the
/// list itself holds only ordered ids, so a realized grid row resolves a position
/// through <c>list.IdAt(pos)</c> then <see cref="AlbumById"/> — the BaeKit shape
/// (ids in the list, rows in a store slice filled by the ingest closure).
///
/// Two refresh channels, matching BaeKit's BrowseListSlot:
/// - a <b>sort change</b> swaps the whole list for a fresh generation-0 instance
///   built over a new page source (<see cref="ListSwapped"/> tells the view to
///   rebind);
/// - a <b>content invalidation</b> (add / remove / reorder) bumps the existing
///   list's generation via <see cref="PaginatedList{TRow, TId}.Invalidate"/>, which
///   the realized rows observe through their load epoch — the list stays put, so
///   scroll and stale rows survive until fresh data replaces them.
///
/// Built once per open library and reset on teardown so nothing bleeds across a
/// library switch. Every mutation runs on the dispatcher thread.
/// </summary>
internal sealed class LibraryBrowserStore
{
    private const int PageSize = 200;

    private readonly LibraryService _library;
    private readonly ImageStore _images;
    private readonly Dispatcher _dispatcher;
    private readonly Action<Exception> _onError;

    private readonly Dictionary<string, Album> _albumsById = new();
    private readonly Dictionary<string, ComposerSummary> _composersById = new();
    private readonly Dictionary<string, ArtistSummary> _artistsById = new();

    private bool _composersLoaded;
    private bool _artistsLoaded;

    public LibrarySort Sort { get; }

    public PaginatedList<Album, string> Albums { get; private set; }
    public PaginatedList<ComposerSummary, string> Composers { get; private set; }
    public PaginatedList<ArtistSummary, string> Artists { get; private set; }

    /// <summary>Raised when a mode's list is swapped for a fresh instance (a sort
    /// change), so the browser view rebinds the active pane to the new list. A
    /// content invalidation does not swap and does not raise this.</summary>
    public event Action<BrowserMode>? ListSwapped;

    public LibraryBrowserStore(
        LibraryService library,
        ImageStore images,
        Dispatcher dispatcher,
        Action<Exception> onError)
    {
        _library = library;
        _images = images;
        _dispatcher = dispatcher;
        _onError = onError;

        Sort = LibrarySortStore.Load();
        Sort.Albums.Changed += () =>
        {
            LibrarySortStore.SaveAlbums(Sort.Albums);
            SwapAlbums();
        };
        Sort.Composers.Changed += () =>
        {
            LibrarySortStore.SaveComposers(Sort.Composers);
            SwapComposers();
        };
        Sort.Artists.Changed += () =>
        {
            LibrarySortStore.SaveArtists(Sort.Artists);
            SwapArtists();
        };

        Albums = BuildAlbumList();
        Composers = BuildComposerList();
        Artists = BuildArtistList();
        // Albums load eagerly at open (the mode shown first), matching BaeKit's
        // LibraryBrowseSession; composers / artists load lazily on first switch.
        _ = Albums.LoadInitialAsync();
    }

    // ── Side-store lookups the grid resolves an id through ────────────────────
    public Album? AlbumById(string id) => _albumsById.GetValueOrDefault(id);

    public ComposerSummary? ComposerById(string id) => _composersById.GetValueOrDefault(id);

    public ArtistSummary? ArtistById(string id) => _artistsById.GetValueOrDefault(id);

    // ── First load / lazy load ────────────────────────────────────────────────

    /// <summary>Kick the active mode's cold load once (albums eagerly at open,
    /// composers/artists on first switch to them). A no-op once a mode has loaded;
    /// a sort change re-loads through the swap path instead.</summary>
    public void EnsureLoaded(BrowserMode mode)
    {
        switch (mode)
        {
            case BrowserMode.Composers when !_composersLoaded:
                _composersLoaded = true;
                _ = Composers.LoadInitialAsync();
                break;
            case BrowserMode.Artists when !_artistsLoaded:
                _artistsLoaded = true;
                _ = Artists.LoadInitialAsync();
                break;
        }
    }

    // ── Content invalidations (core signalled a shape change) ─────────────────
    public void InvalidateAlbums() => Albums.Invalidate();

    public void InvalidateComposers() => Composers.Invalidate();

    public void InvalidateArtists() => Artists.Invalidate();

    // ── Reveal support ────────────────────────────────────────────────────────

    /// <summary>The 0-based position of an album under the active album sort, or
    /// null when it isn't present — lets a reveal page its row in and scroll to
    /// it.</summary>
    public Task<(bool Current, (long? Index, string? Error) Result)> AlbumIndex(string albumId) =>
        _library.AlbumIndex(Sort.Albums.Items, albumId);

    /// <summary>Fetch one album's full detail for the inline expansion.</summary>
    public Task<(bool Current, (AlbumDetail? Detail, string? Error) Response)> AlbumDetail(string albumId) =>
        _library.AlbumDetail(albumId);

    // ── List construction / swap ──────────────────────────────────────────────

    private PaginatedList<Album, string> BuildAlbumList()
    {
        var criteria = Sort.Albums.Items;
        var source = new LibraryPageSource<Album>(
            _library.AlbumCount,
            (offset, limit) =>
            {
                var (current, page) = _library.AlbumPage(offset, limit, criteria);
                return (current, page.Albums, page.Error);
            });
        return new PaginatedList<Album, string>(source, album => album.Id, IngestAlbums, _onError);
    }

    private PaginatedList<ComposerSummary, string> BuildComposerList()
    {
        var criteria = Sort.Composers.Items;
        var source = new LibraryPageSource<ComposerSummary>(
            _library.ComposerCount,
            (offset, limit) =>
            {
                var (current, page) = _library.ComposerPage(offset, limit, criteria);
                return (current, page.Composers, page.Error);
            });
        return new PaginatedList<ComposerSummary, string>(source, composer => composer.ArtistId, IngestComposers, _onError);
    }

    private PaginatedList<ArtistSummary, string> BuildArtistList()
    {
        var criteria = Sort.Artists.Items;
        var source = new LibraryPageSource<ArtistSummary>(
            _library.ArtistCount,
            (offset, limit) =>
            {
                var (current, page) = _library.ArtistPage(offset, limit, criteria);
                return (current, page.Artists, page.Error);
            });
        return new PaginatedList<ArtistSummary, string>(source, artist => artist.ArtistId, IngestArtists, _onError);
    }

    private void SwapAlbums()
    {
        Albums = BuildAlbumList();
        _ = Albums.LoadInitialAsync();
        ListSwapped?.Invoke(BrowserMode.Albums);
    }

    private void SwapComposers()
    {
        Composers = BuildComposerList();
        _composersLoaded = true;
        _ = Composers.LoadInitialAsync();
        ListSwapped?.Invoke(BrowserMode.Composers);
    }

    private void SwapArtists()
    {
        Artists = BuildArtistList();
        _artistsLoaded = true;
        _ = Artists.LoadInitialAsync();
        ListSwapped?.Invoke(BrowserMode.Artists);
    }

    // ── Ingest: attach covers off the UI thread and intern into the side store ─
    private void IngestAlbums(IReadOnlyList<Album> rows)
    {
        foreach (var album in rows)
        {
            album.AttachCover(_images, _dispatcher);
            _albumsById[album.Id] = album;
        }
    }

    private void IngestComposers(IReadOnlyList<ComposerSummary> rows)
    {
        foreach (var composer in rows)
        {
            composer.AttachCover(_images, _dispatcher);
            _composersById[composer.ArtistId] = composer;
        }
    }

    private void IngestArtists(IReadOnlyList<ArtistSummary> rows)
    {
        foreach (var artist in rows)
        {
            artist.AttachCover(_images, _dispatcher);
            _artistsById[artist.ArtistId] = artist;
        }
    }

    /// <summary>The page size a reveal pages a list forward by until the target row
    /// is loaded, and the batch the grid loads around a realized row.</summary>
    public static int Page => PageSize;

    /// <summary>Drop every loaded list and side store on teardown so the next
    /// library opened in this window inherits nothing. Rebuilds fresh
    /// generation-0 lists over the current (about-to-close) session; the window
    /// rebuilds its browser view around them on the next open.</summary>
    public void Reset()
    {
        _albumsById.Clear();
        _composersById.Clear();
        _artistsById.Clear();
        _composersLoaded = false;
        _artistsLoaded = false;
        Albums = BuildAlbumList();
        Composers = BuildComposerList();
        Artists = BuildArtistList();
    }
}
