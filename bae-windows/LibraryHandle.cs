using System;
using System.Threading;
using uniffi.bae_bridge;

namespace Bae.Windows;

internal sealed class LibraryHandle
{
    private readonly ReaderWriterLockSlim _callGate = new();
    private readonly object _stateGate = new();
    private readonly AppHandle _handle;
    private bool _isTearingDown;
    private bool _isFreed;

    internal LibraryHandle(AppHandle handle)
    {
        _handle = handle;
    }

    internal bool IsOpen
    {
        get
        {
            lock (_stateGate)
            {
                return !_isTearingDown && !_isFreed;
            }
        }
    }

    internal bool TryUse(Action<AppHandle> action)
    {
        return TryUse(handle =>
        {
            action(handle);
            return true;
        }, out _);
    }

    internal bool TryUse<T>(Func<AppHandle, T> action, out T result)
    {
        lock (_stateGate)
        {
            if (_isTearingDown || _isFreed)
            {
                result = default!;
                return false;
            }
        }

        _callGate.EnterReadLock();
        try
        {
            lock (_stateGate)
            {
                if (_isTearingDown || _isFreed)
                {
                    result = default!;
                    return false;
                }
            }

            result = action(_handle);
            return true;
        }
        finally
        {
            _callGate.ExitReadLock();
        }
    }

    internal void ShutdownAndFree()
    {
        lock (_stateGate)
        {
            if (_isTearingDown || _isFreed)
            {
                return;
            }

            _isTearingDown = true;
        }

        _callGate.EnterWriteLock();
        try
        {
            NativeBae.Shutdown(_handle);
            NativeBae.HandleFree(_handle);
            lock (_stateGate)
            {
                _isFreed = true;
            }
        }
        finally
        {
            _callGate.ExitWriteLock();
        }
    }
}
