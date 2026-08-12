using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Mirror of core's sync-status snapshot for the toolbar indicator and the sync
// banner. The sync-status value subscription is its only writer; the window
// renders the indicator and banner from these fields on Changed.
internal sealed class SyncStatusStore
{
    private readonly Func<BridgeSyncStatusSnapshot, BridgeSyncIndicator> _indicatorFor;

    // The localized sync-error line, or null when sync is healthy. Kept for the
    // reconnect banner; the toolbar badge is driven by Indicator.
    public string? ErrorText { get; private set; }

    // The badge state, decided by core (error > syncing > synced > idle). The UI
    // maps it to a label and colour and never re-derives which state wins.
    public BridgeSyncIndicator Indicator { get; private set; } = new BridgeSyncIndicator.Idle();

    // The formatted last-sync time, set only when Indicator is Synced — so a stale
    // timestamp can never accompany a stopped loop.
    public string? LastSyncTime { get; private set; }

    public bool? SyncReady { get; private set; }

    public event Action? Changed;

    public SyncStatusStore()
        : this(BaeBridgeMethods.BridgeSyncIndicator)
    {
    }

    internal SyncStatusStore(
        Func<BridgeSyncStatusSnapshot, BridgeSyncIndicator> indicatorFor)
    {
        _indicatorFor = indicatorFor;
    }

    public void Apply(BridgeSyncStatusSnapshot status)
    {
        ErrorText = status.Error is null ? null : BridgeDisplay.LocalizedLine(status.Error);
        Indicator = _indicatorFor(status);
        LastSyncTime = Indicator is BridgeSyncIndicator.Synced synced
            ? SyncIndicatorModel.FormatSyncTime(synced.LastSyncTime)
            : null;
        SyncReady = status.SyncReady;
        Changed?.Invoke();
    }

}
