using System;
using System.Collections.Generic;
using System.Threading.Tasks;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The read behind the import list failing before the list has registered its
/// first page. The consume loop starts with the source, so on a database this
/// build cannot read the failure arrives with nobody to tell — and the page
/// that registers a moment later waits on a loop that has already returned.
/// </summary>
public sealed class ImportListPageSourceFailureTests
{
    private sealed class FailingSubscription : IImportListSubscription
    {
        private readonly TaskCompletionSource _attempted = new();

        public Task FirstReadAttempted => _attempted.Task;

        public Task Cancel() => Task.CompletedTask;

        public Task<BridgeImportListSnapshot> Next()
        {
            _attempted.TrySetResult();
            throw new BridgeException.Diagnostic(
                new BridgeErrorCategory.Database(), "no such column: by_catalog");
        }

        public void SetView(BridgeImportListView view) { }

        public void SetWindows(BridgeLibraryPageWindow[] windows) { }
    }

    private static BridgeImportListView AnyView() => new(
        Tab: BridgeTriageTab.Pending,
        FilterText: string.Empty,
        CollapsedGroups: Array.Empty<BridgeFolderReleaseDecisionKey>(),
        Order: BridgeImportListOrder.PathAscending);

    [Fact]
    public async Task APageRegisteredAfterTheReadFailedIsTold()
    {
        var pending = new List<Action>();
        var subscription = new FailingSubscription();
        var source = new ImportListPageSource(
            AnyView(),
            _ => subscription,
            action => pending.Add(action),
            _ => { });

        // The read fails before anything subscribes, which is the launch race.
        await subscription.FirstReadAttempted;
        // The consume loop hands its failure over the dispatcher; run what it
        // queued, the way the UI thread would.
        await WaitForDispatch(pending);
        RunQueued(pending);

        var failures = new List<Exception>();
        source.Subscribe(0, 10, (_, _) => { }, failures.Add);
        RunQueued(pending);

        var failure = Assert.Single(failures);
        Assert.IsType<PageLoadException>(failure);
    }

    /// The consume loop runs on a thread pool task; give it a moment to reach
    /// the dispatcher rather than assuming it already has.
    private static async Task WaitForDispatch(List<Action> pending)
    {
        for (var attempt = 0; attempt < 200 && pending.Count == 0; attempt++)
        {
            await Task.Delay(10);
        }
    }

    private static void RunQueued(List<Action> pending)
    {
        var queued = pending.ToArray();
        pending.Clear();
        foreach (var action in queued)
        {
            action();
        }
    }
}
