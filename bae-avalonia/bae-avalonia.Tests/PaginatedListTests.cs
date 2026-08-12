using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

public class PaginatedListTests
{
    // Reference-type ids, matching production (album/composer/artist ids are
    // strings), so an unloaded position reads back as null rather than a value
    // type's zero.
    private readonly record struct Row(string Id);

    private static string Id(int i) => $"r{i}";

    private static string[] Ids(params int[] xs) => xs.Select(Id).ToArray();

    // A live page source over a mutable backing list, with call counters and an
    // optional gate so a test can hold the first page delivery open.
    private sealed class FakeSource : IPageSource<Row>
    {
        private sealed record Active(
            int Offset,
            int Limit,
            Action<IReadOnlyList<Row>, int> OnValue,
            Action<Exception> OnError);

        private sealed class Subscription(Action dispose) : IDisposable
        {
            public void Dispose() => dispose();
        }

        private List<Row> _rows;
        private readonly List<Active> _active = new();
        private readonly List<Active> _cancelled = new();
        public int CountCalls;
        public int PageCalls;
        public TaskCompletionSource<bool>? Gate;
        public Exception? CountThrows;
        public int ActivePageCount => _active.Count(active => active.Limit > 0);
        public IReadOnlySet<int> ActivePageOffsets => _active
            .Where(active => active.Limit > 0)
            .Select(active => active.Offset)
            .ToHashSet();

        public FakeSource(int n) => _rows = Enumerable.Range(0, n).Select(i => new Row(Id(i))).ToList();

        public void SetRows(int n)
        {
            _rows = Enumerable.Range(0, n).Select(i => new Row(Id(i))).ToList();
            foreach (var active in _active.ToArray())
            {
                Deliver(active);
            }
        }

        public IDisposable Subscribe(
            int offset,
            int limit,
            Action<IReadOnlyList<Row>, int> onValue,
            Action<Exception> onError)
        {
            if (limit == 0) CountCalls++;
            else PageCalls++;
            var active = new Active(offset, limit, onValue, onError);
            _active.Add(active);
            Deliver(active);
            return new Subscription(() =>
            {
                if (_active.Remove(active))
                {
                    _cancelled.Add(active);
                }
            });
        }

        public void DeliverCancelledValue(int offset, int totalCount)
        {
            var active = _cancelled.First(item => item.Offset == offset);
            active.OnValue(
                Enumerable.Range(offset, active.Limit)
                    .Select(i => new Row($"stale-{Id(i)}"))
                    .ToList(),
                totalCount);
        }

        public void DeliverCancelledError(int offset) =>
            _cancelled.First(item => item.Offset == offset)
                .OnError(new InvalidOperationException("stale error"));

        private void Deliver(Active active)
        {
            if (active.Limit == 0 && CountThrows is { } exception)
            {
                active.OnError(exception);
                return;
            }
            if (Gate is { } gate && active.Limit > 0)
            {
                _ = DeliverAfterGate(active, gate.Task);
                return;
            }
            active.OnValue(_rows.Skip(active.Offset).Take(active.Limit).ToList(), _rows.Count);
        }

        private async Task DeliverAfterGate(Active active, Task gate)
        {
            await gate;
            active.OnValue(_rows.Skip(active.Offset).Take(active.Limit).ToList(), _rows.Count);
        }
    }

    private static PaginatedList<Row, string> Make(FakeSource source, List<Row>? ingested = null, List<Exception>? errors = null) =>
        new(
            source,
            r => r.Id,
            rows => ingested?.AddRange(rows),
            e => errors?.Add(e));

    [Fact]
    public async Task LoadInitial_sets_total_and_clears_segments()
    {
        var source = new FakeSource(42);
        var list = Make(source);
        await list.LoadInitialAsync();

        Assert.Equal(42, list.TotalCount);
        Assert.Null(list.InitialLoadError);
        Assert.Empty(list.AllLoadedIds);
        Assert.Equal(1, source.CountCalls);
    }

    [Fact]
    public async Task LoadInitial_failure_sets_initial_error_not_empty()
    {
        var source = new FakeSource(0) { CountThrows = new InvalidOperationException("db down") };
        var errors = new List<Exception>();
        var list = Make(source, errors: errors);
        await list.LoadInitialAsync();

        // A failed cold load is distinct from an empty library: the initial error
        // is set, and it does not route to the later-page error sink.
        Assert.NotNull(list.InitialLoadError);
        Assert.Empty(errors);
    }

    [Fact]
    public async Task LoadRange_fetches_ingests_and_fills_positions()
    {
        var source = new FakeSource(10);
        var ingested = new List<Row>();
        var list = Make(source, ingested);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 5);

        Assert.Equal(5, ingested.Count);
        Assert.Equal(Ids(0, 1, 2, 3, 4), list.AllLoadedIds);
        Assert.Equal(Id(2), list.IdAt(2));
        Assert.Null(list.IdAt(7)); // not loaded
        Assert.Equal(3, list.PositionOf(Id(3)));
        Assert.Null(list.PositionOf(Id(9)));
    }

    [Fact]
    public async Task LoadRange_fast_path_skips_covered_range()
    {
        var source = new FakeSource(10);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 10);
        Assert.Equal(1, source.PageCalls);

        await list.LoadRangeAsync(2, 3); // already covered by a subscription
        Assert.Equal(1, source.PageCalls);
    }

    [Fact]
    public async Task LoadRange_clamps_to_total_and_noops_past_end()
    {
        var source = new FakeSource(3);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 100);
        Assert.Equal(Ids(0, 1, 2), list.AllLoadedIds);

        await list.LoadRangeAsync(5, 5); // wholly past the end
        Assert.Equal(1, source.PageCalls);
    }

    [Fact]
    public async Task LoadRange_coalesces_concurrent_identical_requests()
    {
        var source = new FakeSource(10) { Gate = new TaskCompletionSource<bool>() };
        var list = Make(source);
        await list.LoadInitialAsync();

        var a = list.LoadRangeAsync(0, 5);
        var b = list.LoadRangeAsync(0, 5);
        source.Gate.SetResult(true);
        await Task.WhenAll(a, b);

        Assert.Equal(1, source.PageCalls); // one shared fetch
    }

    [Fact]
    public async Task Adjacent_segments_merge()
    {
        var source = new FakeSource(10);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 3);
        await list.LoadRangeAsync(3, 3);

        Assert.Equal(Ids(0, 1, 2, 3, 4, 5), list.AllLoadedIds);
        Assert.Equal(Id(5), list.IdAt(5));
    }

    [Fact]
    public async Task Live_values_update_count_and_loaded_rows()
    {
        var source = new FakeSource(10);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 5);
        source.SetRows(20);

        Assert.Equal(20, list.TotalCount);
        Assert.Equal(Ids(0, 1, 2, 3, 4), list.AllLoadedIds);
    }

    [Fact]
    public async Task Live_value_replaces_the_subscribed_segment()
    {
        var source = new FakeSource(10);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 5);

        source.SetRows(8);

        Assert.Equal(Ids(0, 1, 2, 3, 4), list.AllLoadedIds);
        Assert.Equal(1, source.PageCalls);
    }

    [Fact]
    public async Task Shrinking_final_page_removes_ids_beyond_the_new_total()
    {
        var source = new FakeSource(55);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(50, 5);

        source.SetRows(52);

        Assert.Equal(52, list.TotalCount);
        Assert.Equal(Id(50), list.IdAt(50));
        Assert.Equal(Id(51), list.IdAt(51));
        Assert.Null(list.IdAt(52));
        Assert.Equal(Ids(50, 51), list.AllLoadedIds);
    }

    [Fact]
    public async Task Visible_page_subscriptions_stay_bounded_while_scrolling()
    {
        var source = new FakeSource(500);
        var errors = new List<Exception>();
        var list = Make(source, errors: errors);
        await list.LoadInitialAsync();

        for (var offset = 0; offset <= 250; offset += 50)
        {
            await list.LoadRangeAsync(offset, 50);
        }

        Assert.True(source.ActivePageCount <= 3);
        Assert.DoesNotContain(0, source.ActivePageOffsets);
        Assert.Null(list.IdAt(0));

        source.SetRows(501);

        Assert.Equal(501, list.TotalCount);
        Assert.Null(list.IdAt(0));

        await list.LoadRangeAsync(0, 50);
        source.DeliverCancelledValue(0, 999);
        source.DeliverCancelledError(0);

        Assert.Equal(501, list.TotalCount);
        Assert.Equal(Id(0), list.IdAt(0));
        Assert.Empty(errors);
    }

    [Fact]
    public void RowCount_ceils_by_column_count()
    {
        var list = Make(new FakeSource(0));
        list.PreloadForPreview(Enumerable.Range(0, 10).Select(Id).ToList());
        Assert.Equal(4, list.RowCount(3));
        Assert.Equal(2, list.RowCount(5));
        Assert.Equal(0, list.RowCount(0));
    }

    [Fact]
    public void Epoch_differs_across_instances_and_stays_stable_for_values()
    {
        var a = Make(new FakeSource(0));
        var b = Make(new FakeSource(0));
        Assert.NotEqual(a.Epoch, b.Epoch); // distinct instances
        var e0 = a.Epoch;
        a.PreloadForPreview(new List<string> { Id(1) });
        Assert.Equal(e0, a.Epoch);
    }
}
