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
        public Task Cancel() => Task.CompletedTask;

        public Task<BridgeImportListSnapshot> Next() =>
            throw new BridgeException.Diagnostic(
                new BridgeErrorCategory.Database(), "no such column: by_catalog");

        public void SetView(BridgeImportListView view) { }

        public void SetWindows(BridgeLibraryPageWindow[] windows) { }
    }

    /// The dispatcher the source marshals through, which is also how the test
    /// knows the consume loop has reached it — awaited rather than polled, so a
    /// loaded machine cannot turn this into a timeout.
    private sealed class RecordingDispatcher
    {
        private readonly object _gate = new();
        private readonly List<Action> _queued = new();
        private TaskCompletionSource _arrived = new(
            TaskCreationOptions.RunContinuationsAsynchronously);

        public Task Arrived
        {
            get { lock (_gate) { return _arrived.Task; } }
        }

        public void Dispatch(Action action)
        {
            lock (_gate)
            {
                _queued.Add(action);
                _arrived.TrySetResult();
            }
        }

        /// Run what the source queued, the way the UI thread would.
        public void RunQueued()
        {
            Action[] queued;
            lock (_gate)
            {
                queued = _queued.ToArray();
                _queued.Clear();
                _arrived = new TaskCompletionSource(
                    TaskCreationOptions.RunContinuationsAsynchronously);
            }
            foreach (var action in queued)
            {
                action();
            }
        }
    }

    private static BridgeImportListView AnyView() => new(
        Tab: BridgeTriageTab.Pending,
        FilterText: string.Empty,
        CollapsedGroups: Array.Empty<BridgeFolderReleaseDecisionKey>(),
        Order: BridgeImportListOrder.PathAscending);

    [Fact]
    public async Task APageRegisteredAfterTheReadFailedIsTold()
    {
        var dispatcher = new RecordingDispatcher();
        var source = new ImportListPageSource(
            AnyView(),
            _ => new FailingSubscription(),
            dispatcher.Dispatch,
            _ => { });

        // The read fails before anything subscribes, which is the launch race.
        await dispatcher.Arrived;
        dispatcher.RunQueued();

        var failures = new List<Exception>();
        source.Subscribe(0, 10, (_, _) => { }, failures.Add);
        dispatcher.RunQueued();

        var failure = Assert.Single(failures);
        Assert.IsType<PageLoadException>(failure);
    }
}
