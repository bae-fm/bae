using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Linq;
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
    IDisposable Subscribe(
        int offset,
        int limit,
        Action<IReadOnlyList<TRow>, int> onValue,
        Action<Exception> onError);
}

/// <summary>
/// A row's load-task identity: which list epoch and which row position. A view
/// keys a realized slot's load on this so a row's load restarts when its position
/// changes or the list is replaced for a new sort or filter.
/// </summary>
internal readonly record struct LoadEpoch(object Instance);

internal readonly record struct RowLoadId(LoadEpoch Epoch, int Index);

/// <summary>
/// A paginated, ordered view over one or more store slices. Tracks loaded data as
/// a sorted list of non-overlapping segments. Each page subscription delivers
/// its rows and total count whenever its query changes.
/// </summary>
internal sealed class PaginatedList<TRow, TId> : INotifyPropertyChanged
    where TId : notnull
{
    private readonly record struct Segment(int Lower, int Upper, IReadOnlyList<TId> Ids);

    private readonly IPageSource<TRow> _pageSource;
    private readonly Func<TRow, TId> _idOf;
    private readonly Action<IReadOnlyList<TRow>> _ingest;
    // The failure sink for page errors. It takes the exception, not a
    // rendered line: whether a failure is worth showing at all is core's answer
    // (a cancellation is not), and the sink is the one place that drops it.
    private readonly Action<Exception> _onError;

    private readonly List<Segment> _segments = new();
    // Active page subscriptions keyed by offset and limit, so concurrent
    // callers asking for the same range coalesce onto one query instead of each
    // issuing a duplicate — the segment fast-path can't dedupe a burst that starts
    // before any fetch returns.
    private readonly Dictionary<string, IDisposable> _subscriptions = new();
    private readonly Dictionary<string, (int Lower, int Upper)> _subscriptionRanges = new();
    private const int MaximumVisiblePageSubscriptions = 3;
    // A stable per-instance token folded into the epoch so a swapped-in list (fresh
    // instance) is still a distinct epoch.
    private readonly object _identity = new();

    private int _totalCount;
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

    /// <summary>Total row count from the most recent subscription value.</summary>
    public int TotalCount
    {
        get => _totalCount;
        private set => Set(ref _totalCount, value);
    }

    /// <summary>
    /// The cold count load (<see cref="LoadInitialAsync"/>) failed. The grid reads
    /// this to show an error + Retry instead of the empty-library placeholder — a
    /// failed initial load is not an empty library. Only the initial load sets it
    /// (a later page failure keeps data on screen and routes to the error
    /// sink instead); cleared when the initial load starts again or succeeds.
    /// </summary>
    public Exception? InitialLoadError
    {
        get => _initialLoadError;
        private set => Set(ref _initialLoadError, value);
    }

    /// <summary>
    /// This list's epoch. A view folds it into a row's <see cref="RowLoadId"/> so
    /// the row's load restarts when this list is swapped.
    /// </summary>
    public LoadEpoch Epoch => new(_identity);

    // ── Queries ──────────────────────────────────────────────────────────────

    /// <summary>The ID at <paramref name="position"/>, or null when the position
    /// isn't loaded.</summary>
    public TId? IdAt(int position)
    {
        foreach (var seg in _segments)
        {
            if (seg.Lower <= position && position < seg.Upper)
            {
                return seg.Ids[position - seg.Lower];
            }
        }
        return default;
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
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        SubscribeRange(0, 0, initial: true, completion);
        await completion.Task;
    }

    /// <summary>
    /// Load a contiguous range and intern the rows into the store. Fast-path: skips
    /// if an active segment already covers the range. Concurrent callers asking
    /// for the same range share one subscription.
    /// </summary>
    public async Task LoadRangeAsync(int offset, int limit)
    {
        var end = Math.Min(offset + limit, TotalCount);
        if (offset >= end)
        {
            return;
        }
        if (_segments.Any(s => s.Lower <= offset && s.Upper >= end))
        {
            return;
        }
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        SubscribeRange(offset, limit, initial: false, completion);
        await completion.Task;
    }

    private void SubscribeRange(int offset, int limit, bool initial, TaskCompletionSource completion)
    {
        var key = $"{offset}:{limit}";
        if (_subscriptions.ContainsKey(key))
        {
            completion.TrySetResult();
            return;
        }
        try
        {
            _subscriptions[key] = _pageSource.Subscribe(
                offset,
                limit,
                (rows, totalCount) =>
                {
                    TotalCount = totalCount;
                    ClipSegments(totalCount);
                    InitialLoadError = null;
                    if (!initial)
                    {
                        _ingest(rows);
                        InsertSegment(new Segment(offset, offset + rows.Count, rows.Select(_idOf).ToList()));
                        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(Epoch)));
                    }
                    completion.TrySetResult();
                },
                exception =>
                {
                    if (initial)
                    {
                        InitialLoadError = exception;
                    }
                    else
                    {
                        _onError(exception);
                    }
                    completion.TrySetResult();
                });
            if (!initial)
            {
                _subscriptionRanges[key] = (offset, offset + limit);
                EvictPagesOutsideWindow(offset, offset + limit);
            }
        }
        catch (Exception exception)
        {
            if (initial) InitialLoadError = exception;
            else _onError(exception);
            completion.TrySetResult();
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
            if (seg.Upper >= lower && seg.Lower <= upper)
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
        remaining.Add(new Segment(lower, upper, merged));
        remaining.Sort((a, b) => a.Lower.CompareTo(b.Lower));

        _segments.Clear();
        _segments.AddRange(remaining);
        ClipSegments(TotalCount);
    }

    private void ClipSegments(int totalCount)
    {
        for (var index = _segments.Count - 1; index >= 0; index--)
        {
            var segment = _segments[index];
            var upper = Math.Min(segment.Upper, totalCount);
            if (segment.Lower >= upper)
            {
                _segments.RemoveAt(index);
            }
            else if (upper < segment.Upper)
            {
                _segments[index] = new Segment(
                    segment.Lower,
                    upper,
                    segment.Ids.Take(upper - segment.Lower).ToList());
            }
        }
    }

    private void EvictPagesOutsideWindow(int visibleLower, int visibleUpper)
    {
        var center = visibleLower + (visibleUpper - visibleLower) / 2;
        while (_subscriptionRanges.Count > MaximumVisiblePageSubscriptions)
        {
            var key = _subscriptionRanges.MaxBy(pair =>
                Math.Abs(pair.Value.Lower + (pair.Value.Upper - pair.Value.Lower) / 2 - center)).Key;
            var range = _subscriptionRanges[key];
            _subscriptionRanges.Remove(key);
            _subscriptions.Remove(key, out var subscription);
            subscription!.Dispose();
            RemoveLoadedRange(range.Lower, range.Upper);
        }
    }

    private void RemoveLoadedRange(int lower, int upper)
    {
        var remaining = new List<Segment>();
        foreach (var segment in _segments)
        {
            if (segment.Upper <= lower || segment.Lower >= upper)
            {
                remaining.Add(segment);
                continue;
            }
            if (segment.Lower < lower)
            {
                remaining.Add(new Segment(
                    segment.Lower,
                    lower,
                    segment.Ids.Take(lower - segment.Lower).ToList()));
            }
            if (segment.Upper > upper)
            {
                remaining.Add(new Segment(
                    upper,
                    segment.Upper,
                    segment.Ids.Skip(upper - segment.Lower).ToList()));
            }
        }
        _segments.Clear();
        _segments.AddRange(remaining);
    }

    // ── Test / preview support ───────────────────────────────────────────────

    public void Cancel()
    {
        foreach (var subscription in _subscriptions.Values)
        {
            subscription.Dispose();
        }
        _subscriptions.Clear();
        _subscriptionRanges.Clear();
    }

    /// <summary>Seed one segment synchronously for previews and tests.</summary>
    public void PreloadForPreview(IReadOnlyList<TId> ids)
    {
        _segments.Clear();
        _segments.Add(new Segment(0, ids.Count, ids));
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
