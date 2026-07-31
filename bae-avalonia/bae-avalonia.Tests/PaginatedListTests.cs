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

    // A page source over a fixed backing list, with a count that can change to
    // simulate a shape change, call counters, and an optional gate so a test can
    // hold a fetch open to exercise coalescing and stale-generation drops.
    private sealed class FakeSource : IPageSource<Row>
    {
        private List<Row> _rows;
        public int CountCalls;
        public int PageCalls;
        public TaskCompletionSource<bool>? Gate;
        public Exception? CountThrows;

        public FakeSource(int n) => _rows = Enumerable.Range(0, n).Select(i => new Row(Id(i))).ToList();

        public void SetRows(int n) => _rows = Enumerable.Range(0, n).Select(i => new Row(Id(i))).ToList();

        public Task<int> CountAsync()
        {
            CountCalls++;
            return CountThrows is { } ex ? Task.FromException<int>(ex) : Task.FromResult(_rows.Count);
        }

        public async Task<IReadOnlyList<Row>> PageAsync(int offset, int limit)
        {
            PageCalls++;
            if (Gate is { } gate)
            {
                await gate.Task;
            }
            return _rows.Skip(offset).Take(limit).ToList();
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
        // is set, and it does NOT route to the page/invalidate error sink.
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

        await list.LoadRangeAsync(2, 3); // already covered at the current generation
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
    public async Task Adjacent_same_generation_segments_merge()
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
    public async Task Invalidate_recounts_and_bumps_generation()
    {
        var source = new FakeSource(10);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 5);
        var gen0 = list.Generation;

        source.SetRows(20);
        list.Invalidate();
        await list.AwaitReloadAsync();

        Assert.Equal(20, list.TotalCount);
        Assert.Equal(gen0 + 1, list.Generation);
        Assert.Equal(Ids(0, 1, 2, 3, 4), list.AllLoadedIds); // stale segment stays visible
    }

    [Fact]
    public async Task Stale_generation_fetch_result_is_dropped()
    {
        var source = new FakeSource(10) { Gate = new TaskCompletionSource<bool>() };
        var ingested = new List<Row>();
        var list = Make(source, ingested);
        await list.LoadInitialAsync();

        var pending = list.LoadRangeAsync(0, 5);
        list.Invalidate();
        await list.AwaitReloadAsync();
        source.Gate.SetResult(true);
        await pending;

        Assert.Empty(ingested); // fetched at the old generation → dropped
        Assert.Empty(list.AllLoadedIds);
    }

    [Fact]
    public async Task Reload_after_invalidate_replaces_stale_segment()
    {
        var source = new FakeSource(10);
        var list = Make(source);
        await list.LoadInitialAsync();
        await list.LoadRangeAsync(0, 5);

        source.SetRows(8);
        list.Invalidate();
        await list.AwaitReloadAsync();
        await list.LoadRangeAsync(0, 5); // re-fetch at the new generation

        Assert.Equal(Ids(0, 1, 2, 3, 4), list.AllLoadedIds);
        Assert.Equal(2, source.PageCalls);
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
    public void Epoch_differs_across_instances_and_generations()
    {
        var a = Make(new FakeSource(0));
        var b = Make(new FakeSource(0));
        Assert.NotEqual(a.Epoch, b.Epoch); // distinct instances
        var e0 = a.Epoch;
        a.PreloadForPreview(new List<string> { Id(1) });
        Assert.Equal(e0, a.Epoch); // preview doesn't bump the generation
    }
}
