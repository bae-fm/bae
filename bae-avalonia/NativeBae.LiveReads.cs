using uniffi.bae_bridge;

namespace Bae.Desktop;

// A reconfigurable bridge subscription read as a plain callback stream: every
// value goes to onValue until the subscription is cancelled or a read fails.
internal static partial class NativeBae
{
    private static IDisposable ReadEachValue<T>(
        IDisposable subscription,
        Func<Task> cancel,
        Func<Task<T>> next,
        Action<T> onValue,
        Action<Exception> onError)
    {
        _ = Task.Run(async () =>
        {
            while (true)
            {
                T value;
                try
                {
                    value = await next();
                }
                catch (BridgeException.Cancelled)
                {
                    return;
                }
                catch (BridgeException error)
                {
                    onError(error);
                    return;
                }
                onValue(value);
            }
        });
        return new LiveRead(subscription, cancel);
    }

    private sealed class LiveRead(IDisposable subscription, Func<Task> cancel) : IDisposable
    {
        public void Dispose()
        {
            // Cancelling settles the pending read so the loop ends; freeing the
            // object afterwards releases what core held for it.
            _ = cancel();
            subscription.Dispose();
        }
    }
}
