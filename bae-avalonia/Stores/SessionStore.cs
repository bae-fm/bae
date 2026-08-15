using System;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// The outcome of opening a library handle: ready to use, locked (encrypted with
// the key absent on this device), or failed to open at all.
internal enum OpenHandleResult
{
    Opened,
    NeedsUnlock,
    Failed,
}

// Owns the open library's handle and the event subscription. The handle is held
// for the session's lifetime (playback, events, and screens reuse it) and freed
// on teardown. A monotonic session generation, bumped on every open and free,
// fences stale event deliveries: an event whose generation no longer matches the
// current one is dropped before it reaches the UI.
internal sealed class SessionStore
{
    private const uint PositionUpdateIntervalMs = 250;

    private readonly object _handleGate = new();
    private readonly Dispatcher _dispatcher;
    private LibraryHandle? _handle;
    private LibraryHandle? _lockedHandle;
    private int _sessionGeneration;

    // Held for the subscription's lifetime; the generated callback map keeps the
    // managed object rooted while Rust holds the callback handle.
    private NativeBae.UiEventSink? _eventCallback;

    // Raised on the UI thread for events belonging to the current session; stale
    // events (from a freed or since-replaced handle) are filtered out first.
    public event Action<BridgeUiEvent>? UiEvent;

    public SessionStore(Dispatcher dispatcher)
    {
        _dispatcher = dispatcher;
    }

    // Open the library's handle. A locked handle remains owned here so the
    // unlock operation completes that same Coven owner.
    public OpenHandleResult OpenHandle(string libraryId)
    {
        // The restore-on-launch preference gates the startup restore in the core;
        // the resume row itself is written continuously either way.
        var handle = NativeBae.Init(
            libraryId,
            PositionUpdateIntervalMs,
            PersistPlaybackStore.Load(),
            // The telemetry sink built at startup; init_app requires it, so
            // telemetry is guaranteed up before the library opens.
            BaeDiagnostics.Handle);
        if (handle == null)
        {
            return OpenHandleResult.Failed;
        }

        if (NativeBae.CloudHomeKeyState(handle) == BridgeCloudHomeKeyState.Locked)
        {
            lock (_handleGate)
            {
                _lockedHandle?.ShutdownAndFree();
                _lockedHandle = new LibraryHandle(handle);
                _sessionGeneration++;
            }
            return OpenHandleResult.NeedsUnlock;
        }

        lock (_handleGate)
        {
            _handle = new LibraryHandle(handle);
            _sessionGeneration++;
        }

        return OpenHandleResult.Opened;
    }

    public async System.Threading.Tasks.Task<string?> Unlock(string serializedMasterKey)
    {
        LibraryHandle pending;
        int generation;
        lock (_handleGate)
        {
            if (_lockedHandle == null)
            {
                throw new System.OperationCanceledException();
            }

            pending = _lockedHandle;
            generation = _sessionGeneration;
        }

        var response = await System.Threading.Tasks.Task.Run(() =>
        {
            return pending.TryUse(
                handle => NativeBae.UnlockCloudHome(handle, serializedMasterKey),
                out var error)
                ? error
                : throw new System.OperationCanceledException();
        });
        if (response is not null)
        {
            return response;
        }

        lock (_handleGate)
        {
            if (_lockedHandle != pending || _sessionGeneration != generation)
            {
                throw new System.OperationCanceledException();
            }

            _lockedHandle = null;
            _handle = pending;
            _sessionGeneration++;
        }
        return null;
    }

    public async System.Threading.Tasks.Task CancelUnlock()
    {
        LibraryHandle? pending;
        lock (_handleGate)
        {
            pending = _lockedHandle;
            _lockedHandle = null;
            if (pending is not null)
            {
                _sessionGeneration++;
            }
        }
        if (pending is not null)
        {
            await System.Threading.Tasks.Task.Run(pending.ShutdownAndFree);
        }
    }

    // Subscribe to core's UI events for the current session. The generation is
    // captured now so a delivery arriving after a switch or close is dropped.
    public void Subscribe()
    {
        var generation = _sessionGeneration;
        _eventCallback = new NativeBae.UiEventSink(evt => OnNativeEvent(generation, evt));
        WithCurrentHandle(handle => NativeBae.Subscribe(handle, _eventCallback));
    }

    // Fires on a background thread; hop to the UI thread, then drop the event if
    // its session has been torn down or replaced before touching the UI.
    private void OnNativeEvent(int generation, BridgeUiEvent evt) =>
        _dispatcher.Post(() =>
        {
            if (generation != _sessionGeneration || CurrentHandleOrNull() == null)
            {
                return;
            }

            UiEvent?.Invoke(evt);
        });

    public async System.Threading.Tasks.Task ShutdownAndFreeCurrentHandle()
    {
        LibraryHandle? handle;
        LibraryHandle? lockedHandle;
        lock (_handleGate)
        {
            if (_handle == null && _lockedHandle == null)
            {
                return;
            }

            handle = _handle;
            lockedHandle = _lockedHandle;
            _handle = null;
            _lockedHandle = null;
            _sessionGeneration++;
        }

        await System.Threading.Tasks.Task.Run(() =>
        {
            handle?.ShutdownAndFree();
            lockedHandle?.ShutdownAndFree();
        });
        _eventCallback = null;
    }

    public async System.Threading.Tasks.Task<(bool Current, T Result)>
        RunForCurrentHandle<T>(Func<AppHandle, T> action)
    {
        var response = await System.Threading.Tasks.Task.Run(() =>
        {
            LibraryHandle handle;
            int generation;
            lock (_handleGate)
            {
                if (_handle == null)
                {
                    return (Ran: false, Generation: _sessionGeneration, Result: default!);
                }

                handle = _handle;
                generation = _sessionGeneration;
            }

            return handle.TryUse(action, out var result)
                ? (Ran: true, Generation: generation, Result: result)
                : (Ran: false, Generation: generation, Result: default!);
        });
        return (
            response.Ran && response.Generation == _sessionGeneration,
            response.Result);
    }

    public async System.Threading.Tasks.Task<bool> RunForCurrentHandle(Action<AppHandle> action)
    {
        var (current, _) = await RunForCurrentHandle(handle =>
        {
            action(handle);
            return true;
        });
        return current;
    }

    public bool WithCurrentHandle(Action<AppHandle> action)
    {
        LibraryHandle handle;
        lock (_handleGate)
        {
            if (_handle == null)
            {
                return false;
            }

            handle = _handle;
        }

        return handle.TryUse(action);
    }

    public (bool Current, T Result) WithCurrentHandle<T>(Func<AppHandle, T> action)
    {
        LibraryHandle handle;
        lock (_handleGate)
        {
            if (_handle == null)
            {
                return (false, default!);
            }

            handle = _handle;
        }

        return handle.TryUse(action, out var result)
            ? (true, result)
            : (false, default!);
    }

    public LibraryHandle? CurrentHandleOrNull()
    {
        lock (_handleGate)
        {
            return _handle is { IsOpen: true } ? _handle : null;
        }
    }
}
