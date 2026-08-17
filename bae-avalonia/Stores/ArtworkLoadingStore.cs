using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>The retained post-open CacheEager artwork operation.</summary>
internal sealed class ArtworkLoadingStore
{
    private readonly Action _cancel;

    public ArtworkLoadingStore(Action cancel)
    {
        _cancel = cancel;
    }

    public BridgeEagerCacheFillStatus Status { get; private set; }
        = new BridgeEagerCacheFillStatus.NotRunning();

    public event Action? Changed;

    public void Apply(BridgeEagerCacheFillStatus status)
    {
        Status = status;
        Changed?.Invoke();
    }

    public void Cancel() => _cancel();
}
