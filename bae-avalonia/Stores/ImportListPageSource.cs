using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// An <see cref="IPageSource{TRow}"/> over the import tab's one reconfigurable
/// subscription. Every other page source opens a query per page; this one holds
/// a single list subscription and multiplexes the pages onto it: each page
/// registers a sink under its (offset, limit) and the whole set of registered
/// windows is handed to <c>SetWindows</c>, so core reads the queue once per
/// change and answers every window in one value.
///
/// One background loop drives <c>Next()</c>. Each value is marshalled to the UI
/// thread, where its windows are matched back to the sinks that asked for them
/// and the chrome around the list goes to <paramref name="onSnapshot"/>.
/// </summary>
internal sealed class ImportListPageSource : IPageSource<BridgeImportListItem>, IDisposable
{
    private readonly Action<Action> _dispatch;
    private readonly Action<BridgeImportListSnapshot> _onSnapshot;
    private readonly Dictionary<(int Offset, int Limit), Action<IReadOnlyList<BridgeImportListItem>, int>> _sinks = new();
    private readonly Dictionary<(int Offset, int Limit), Action<Exception>> _failures = new();
    private readonly IImportListSubscription? _subscription;

    private bool _closed;

    public ImportListPageSource(
        BridgeImportListView view,
        Func<BridgeImportListView, IImportListSubscription?> subscribe,
        Action<Action> dispatch,
        Action<BridgeImportListSnapshot> onSnapshot)
    {
        _dispatch = dispatch;
        _onSnapshot = onSnapshot;
        _subscription = subscribe(view);
        if (_subscription is not null)
        {
            _ = Task.Run(Consume);
        }
    }

    private ImportListPageSource()
    {
        _dispatch = action => action();
        _onSnapshot = _ => { };
        _closed = true;
    }

    /// <summary>A source with nothing behind it: the list before a library is
    /// open, and the seeded lists the previews and view tests render. Every
    /// page it is asked for is a read that was never going to happen.</summary>
    public static ImportListPageSource Closed() => new();

    /// <summary>Show a different tab, filter, order, or set of folded groups.
    /// The windows are kept: the query reruns and every registered page
    /// re-ingests its rows at the same offsets.</summary>
    public void SetView(BridgeImportListView view)
    {
        if (_closed || _subscription is null)
        {
            return;
        }
        try
        {
            _subscription.SetView(view);
        }
        catch (BridgeException)
        {
            // The subscription is gone; the list is being replaced with it.
            _closed = true;
        }
    }

    public IDisposable Subscribe(
        int offset,
        int limit,
        Action<IReadOnlyList<BridgeImportListItem>, int> onValue,
        Action<Exception> onError)
    {
        if (_closed || _subscription is null)
        {
            throw new OperationCanceledException();
        }
        var key = (offset, limit);
        _sinks[key] = onValue;
        _failures[key] = onError;
        PushWindows();
        return new WindowRegistration(this, key);
    }

    private void Release((int Offset, int Limit) key)
    {
        if (!_sinks.Remove(key))
        {
            return;
        }
        _failures.Remove(key);
        PushWindows();
    }

    private void PushWindows()
    {
        if (_closed || _subscription is null)
        {
            return;
        }
        var windows = _sinks.Keys
            .Select(key => new BridgeLibraryPageWindow((ulong)key.Offset, (ulong)key.Limit))
            .ToArray();
        try
        {
            _subscription.SetWindows(windows);
        }
        catch (BridgeException error)
        {
            _closed = true;
            Fail(new PageLoadException(BridgeDisplay.LocalizedLine(error)));
        }
    }

    private async Task Consume()
    {
        while (true)
        {
            BridgeImportListSnapshot snapshot;
            try
            {
                snapshot = await _subscription!.Next();
            }
            catch (BridgeException.Cancelled)
            {
                return;
            }
            catch (BridgeException error)
            {
                var line = BridgeDisplay.LocalizedLine(error);
                _dispatch(() => Fail(new PageLoadException(line)));
                return;
            }
            _dispatch(() => Deliver(snapshot));
        }
    }

    private void Deliver(BridgeImportListSnapshot snapshot)
    {
        _onSnapshot(snapshot);
        var total = checked((int)snapshot.TotalCount);
        foreach (var window in snapshot.Windows)
        {
            var key = (checked((int)window.Window.Offset), checked((int)window.Window.Limit));
            if (_sinks.TryGetValue(key, out var sink))
            {
                sink(window.Items, total);
            }
        }
    }

    private void Fail(Exception error)
    {
        foreach (var failure in _failures.Values.ToList())
        {
            failure(error);
        }
    }

    public void Dispose()
    {
        _closed = true;
        _sinks.Clear();
        _failures.Clear();
        if (_subscription is null)
        {
            return;
        }
        // Cancelling settles the pending Next so the loop ends; freeing the
        // object afterwards releases what core held for it.
        _ = _subscription.Cancel();
        if (_subscription is IDisposable disposable)
        {
            disposable.Dispose();
        }
    }

    private sealed class WindowRegistration(
        ImportListPageSource source,
        (int Offset, int Limit) key) : IDisposable
    {
        public void Dispose() => source.Release(key);
    }
}
