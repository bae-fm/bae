using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;

namespace Bae.Desktop;

/// <summary>
/// Source of a paginated stream of rows: knows how to count and fetch contiguous
/// pages for one query (sort + scope). Paired with exactly one
/// <see cref="PaginatedList{TRow, TId}"/>; the scope is baked into the source, not
/// configured on the list.
/// </summary>
internal interface IPageSource<TRow>
{
    /// <summary>Total row count for this query.</summary>
    Task<int> CountAsync();

    /// <summary>Fetch a contiguous page of rows starting at <paramref name="offset"/>.</summary>
    Task<IReadOnlyList<TRow>> PageAsync(int offset, int limit);
}

/// <summary>
/// A row's load-task identity: which list epoch and which row position. A view
/// keys a realized slot's load on this so a row's load restarts when its position
/// changes, or when the list is invalidated (generation bumps) or swapped for a
/// fresh instance (a new sort/filter builds a new list at generation 0). Position
/// or generation alone misses the swap — a fresh instance can sit at the same
/// generation — so the instance identity closes that gap.
/// </summary>
internal readonly record struct LoadEpoch(object Instance, int Generation);

internal readonly record struct RowLoadId(LoadEpoch Epoch, int Index);

/// <summary>
/// A paginated, ordered view over one or more store slices. Tracks loaded data as
/// a sorted list of non-overlapping segments, each tagged with the generation at
/// which it was fetched — no pre-allocation, only loaded positions are stored.
///
/// Shape changes (add / remove / reorder) go through <see cref="Invalidate"/>,
/// which re-counts and bumps the generation. Visible rows restart their load
/// tasks (keyed on <see cref="Epoch"/>) and call <see cref="LoadRangeAsync"/>,
/// which re-fetches any range whose cached segment is stale; stale segments keep
/// their data visible until fresh data replaces them, so the grid never flashes
/// to empty.
///
/// The list observes nothing. Content mutations re-render through the store slices
/// the rows live in; shape mutations require an explicit <see cref="Invalidate"/>
/// from the action handler. Mutating methods run on the dispatcher thread (the app
/// serializes them there); the paging state is otherwise pure.
/// </summary>
internal sealed class PaginatedList<TRow, TId> : INotifyPropertyChanged
    where TId : notnull
{
    private readonly record struct Segment(int Lower, int Upper, IReadOnlyList<TId> Ids, int Generation);

    private readonly IPageSource<TRow> _pageSource;
    private readonly Func<TRow, TId> _idOf;
    private readonly Action<IReadOnlyList<TRow>> _ingest;
    // The failure sink for page / invalidate errors. It takes the exception, not a
    // rendered line: whether a failure is worth showing at all is core's answer
    // (a cancellation is not), and the sink is the one place that drops it.
    private readonly Action<Exception> _onError;

    private readonly List<Segment> _segments = new();
    // In-flight LoadRange fetches keyed "offset:end:generation", so concurrent
    // callers asking for the same range coalesce onto one query instead of each
    // issuing a duplicate — the segment fast-path can't dedupe a burst that starts
    // before any fetch returns.
    private readonly Dictionary<string, Task> _inFlight = new();
    // A stable per-instance token folded into the epoch so a swapped-in list (fresh
    // instance, same generation) is still a distinct epoch.
    private readonly object _identity = new();

    private CancellationTokenSource? _reloadCts;
    private Task _reloadTask = Task.CompletedTask;

    private int _totalCount;
    private int _generation;
    private Exception? _initialLoadError;

    public PaginatedList(
        IPageSource<TRow> pageSource,
        Func<TRow, TId> idOf,
        Action<IReadOnlyList<TRow>> ingest,
        Action<Exception> onError)
    {
        _pageSource = pageSource;
        _idOf = idOf;
        _ingest = ingest;
        _onError = onError;
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    /// <summary>Total row count from the most recent load or invalidate.</summary>
    public int TotalCount
    {
        get => _totalCount;
        private set => Set(ref _totalCount, value);
    }

    /// <summary>Bumped by <see cref="Invalidate"/>; folded into <see cref="Epoch"/>.</summary>
    public int Generation
    {
        get => _generation;
        private set => Set(ref _generation, value);
    }

    /// <summary>
    /// The cold count load (<see cref="LoadInitialAsync"/>) failed. The grid reads
    /// this to show an error + Retry instead of the empty-library placeholder — a
    /// failed initial load is not an empty library. Only the initial load sets it
    /// (a page or invalidate failure keeps data on screen and routes to the error
    /// sink instead); cleared when the initial load starts again or succeeds.
    /// </summary>
    public Exception? InitialLoadError
    {
        get => _initialLoadError;
        private set => Set(ref _initialLoadError, value);
    }

    /// <summary>
    /// This list's epoch. A view folds it into a row's <see cref="RowLoadId"/> so
    /// the row's load restarts when this list is invalidated or swapped.
    /// </summary>
    public LoadEpoch Epoch => new(_identity, Generation);

    // ── Queries ──────────────────────────────────────────────────────────────

    /// <summary>The ID at <paramref name="position"/>, preferring the highest-
    /// generation segment so fresh data wins over stale when they transiently
    /// overlap; null when the position isn't loaded.</summary>
    public TId? IdAt(int position)
    {
        Segment? best = null;
        foreach (var seg in _segments)
        {
            if (seg.Lower <= position && position < seg.Upper
                && (best is null || seg.Generation > best.Value.Generation))
            {
                best = seg;
            }
        }
        return best is { } b ? b.Ids[position - b.Lower] : default;
    }

    /// <summary>The position of <paramref name="id"/> in the loaded segments, or
    /// null if not loaded.</summary>
    public int? PositionOf(TId id)
    {
        foreach (var seg in _segments)
        {
            var local = -1;
            for (var i = 0; i < seg.Ids.Count; i++)
            {
                if (EqualityComparer<TId>.Default.Equals(seg.Ids[i], id))
                {
                    local = i;
                    break;
                }
            }
            if (local >= 0)
            {
                return seg.Lower + local;
            }
        }
        return null;
    }

    /// <summary>All IDs currently held in loaded segments, in order.</summary>
    public IReadOnlyList<TId> AllLoadedIds => _segments.SelectMany(s => s.Ids).ToList();

    /// <summary>Row count for a grid layout with the given column count.</summary>
    public int RowCount(int columnCount) =>
        columnCount <= 0 ? 0 : (TotalCount + columnCount - 1) / columnCount;

    // ── Load API ─────────────────────────────────────────────────────────────

    /// <summary>Fetch the total count. Called once when the list is first mounted.</summary>
    public async Task LoadInitialAsync()
    {
        InitialLoadError = null;
        var gen = Generation;
        try
        {
            var count = await _pageSource.CountAsync();
            if (Generation != gen)
            {
                return;
            }
            TotalCount = count;
            _segments.Clear();
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception exception)
        {
            // A cold load with nothing on screen: surface it as the list's failed
            // state so the grid shows error + Retry instead of the empty-library
            // placeholder. The error sink (the banner) is reserved for page /
            // invalidate failures, which leave existing rows visible.
            InitialLoadError = exception;
        }
    }

    /// <summary>
    /// Load a contiguous range and intern the rows into the store. Fast-path: skips
    /// if a current-generation segment already covers the range. Concurrent callers
    /// asking for the same range + generation coalesce onto one in-flight fetch.
    /// </summary>
    public async Task LoadRangeAsync(int offset, int limit)
    {
        var end = Math.Min(offset + limit, TotalCount);
        if (offset >= end)
        {
            return;
        }
        var gen = Generation;

        if (_segments.Any(s => s.Generation == gen && s.Lower <= offset && s.Upper >= end))
        {
            return;
        }

        var key = $"{offset}:{end}:{gen}";
        if (_inFlight.TryGetValue(key, out var existing))
        {
            await existing;
            return;
        }

        var task = FetchRangeAsync(offset, end, limit, gen);
        _inFlight[key] = task;
        try
        {
            await task;
        }
        finally
        {
            _inFlight.Remove(key);
        }
    }

    private async Task FetchRangeAsync(int offset, int end, int limit, int gen)
    {
        try
        {
            var rows = await _pageSource.PageAsync(offset, limit);
            // Drop the result if the generation advanced while fetching — the
            // restarted task re-fetches at the new generation.
            if (Generation != gen)
            {
                return;
            }
            _ingest(rows);
            InsertSegment(new Segment(offset, end, rows.Select(_idOf).ToList(), gen));
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception exception)
        {
            // The failure sink surfaces it (banner + log); a stale range leaves the
            // existing rows on screen.
            _onError(exception);
        }
    }

    // ── Invalidation ─────────────────────────────────────────────────────────

    /// <summary>Mark the list stale: re-count and bump the generation. Visible rows
    /// restart their load tasks and re-fetch; stale segments stay visible until
    /// replaced.</summary>
    public void Invalidate()
    {
        _reloadCts?.Cancel();
        _reloadCts?.Dispose();
        var cts = new CancellationTokenSource();
        _reloadCts = cts;
        _reloadTask = ReloadAsync(cts.Token);
    }

    private async Task ReloadAsync(CancellationToken token)
    {
        try
        {
            var newCount = await _pageSource.CountAsync();
            token.ThrowIfCancellationRequested();
            TotalCount = newCount;
            Generation += 1;
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception exception)
        {
            _onError(exception);
        }
    }

    // ── Segment management ───────────────────────────────────────────────────

    private void InsertSegment(Segment @new)
    {
        var lower = @new.Lower;
        var upper = @new.Upper;
        var leftIds = new List<TId>();
        var rightIds = new List<TId>();
        var remaining = new List<Segment>();

        foreach (var seg in _segments)
        {
            if (seg.Generation == @new.Generation && seg.Upper >= lower && seg.Lower <= upper)
            {
                // Same-gen, touches or overlaps: absorb the parts beyond [lower, upper].
                if (seg.Lower < lower)
                {
                    leftIds = seg.Ids.Take(lower - seg.Lower).Concat(leftIds).ToList();
                    lower = seg.Lower;
                }
                if (seg.Upper > upper)
                {
                    var take = seg.Upper - upper;
                    rightIds.AddRange(seg.Ids.Skip(seg.Ids.Count - take));
                    upper = seg.Upper;
                }
                // The portion within [lower, upper] is superseded by new.Ids.
            }
            else if (seg.Upper > @new.Lower && seg.Lower < @new.Upper)
            {
                // Stale segment overlaps the freshly-fetched range — discard.
            }
            else
            {
                remaining.Add(seg);
            }
        }

        var merged = leftIds.Concat(@new.Ids).Concat(rightIds).ToList();
        remaining.Add(new Segment(lower, upper, merged, @new.Generation));
        remaining.Sort((a, b) => a.Lower.CompareTo(b.Lower));

        _segments.Clear();
        _segments.AddRange(remaining);
    }

    // ── Test / preview support ───────────────────────────────────────────────

    /// <summary>Await the in-flight reload task, so a test can synchronize on
    /// post-<see cref="Invalidate"/> state without racing.</summary>
    public Task AwaitReloadAsync() => _reloadTask;

    /// <summary>Seed one segment synchronously for previews and tests.</summary>
    public void PreloadForPreview(IReadOnlyList<TId> ids)
    {
        _segments.Clear();
        _segments.Add(new Segment(0, ids.Count, ids, Generation));
        TotalCount = ids.Count;
    }

    private void Set<T>(ref T field, T value, [System.Runtime.CompilerServices.CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value))
        {
            return;
        }
        field = value;
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name));
    }
}
