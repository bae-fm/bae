using System;
using Avalonia.Threading;

namespace Bae.Desktop;

internal sealed class LatestUiValue<T>(Action<T> apply)
{
    private readonly object _gate = new();
    private T _latest = default!;
    private bool _hasValue;
    private bool _scheduled;

    public void Offer(T value)
    {
        lock (_gate)
        {
            _latest = value;
            _hasValue = true;
            if (_scheduled)
            {
                return;
            }
            _scheduled = true;
        }
        Dispatcher.UIThread.Post(Deliver);
    }

    private void Deliver()
    {
        T value;
        lock (_gate)
        {
            value = _latest;
            _hasValue = false;
            _scheduled = false;
        }
        apply(value);

        lock (_gate)
        {
            if (!_hasValue || _scheduled)
            {
                return;
            }
            _scheduled = true;
        }
        Dispatcher.UIThread.Post(Deliver);
    }
}
