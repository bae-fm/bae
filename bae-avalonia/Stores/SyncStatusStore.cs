using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Mirror of core's sync-status snapshot for the toolbar indicator and the sync
// banner. The sync-status value subscription is its only writer; the window
// renders the indicator and banner from these fields on Changed.
internal sealed class SyncStatusStore
{
    private readonly Func<BridgeSyncStatusSnapshot, BridgeSyncIndicator> _indicatorFor;
    private readonly Func<BridgeException, string?> _lineFor;

    // The localized sync-error line, or null when sync is healthy. Read by the
    // sync failure row; the toolbar badge is driven by Indicator.
    public string? ErrorText { get; private set; }

    // The concrete fault behind ErrorText — the untranslated chain core recorded,
    // as one line. ErrorText names only a category ("Something went wrong."), so
    // the row renders this under it; null when the failure carries no diagnostic.
    public string? ErrorDetail { get; private set; }

    // The badge state, decided by core (error > syncing > synced > idle). The UI
    // maps it to a label and colour and never re-derives which state wins.
    public BridgeSyncIndicator Indicator { get; private set; } = new BridgeSyncIndicator.Idle();

    // The formatted last-sync time, set only when Indicator is Synced — so a stale
    // timestamp can never accompany a stopped loop.
    public string? LastSyncTime { get; private set; }

    public bool CanReconnect { get; private set; }

    public bool? SyncReady { get; private set; }

    // The durable sync operations the last completed cycle left waiting on a
    // person. Empty while there is nothing waiting; each is retried by handing
    // its Id back through SyncService.RetryBlockedSyncOperation.
    public IReadOnlyList<BridgeBlockedSyncOperation> Blocked { get; private set; } =
        Array.Empty<BridgeBlockedSyncOperation>();

    public event Action? Changed;

    public SyncStatusStore()
        : this(BaeBridgeMethods.BridgeSyncIndicator, BridgeDisplay.LocalizedLine)
    {
    }

    internal SyncStatusStore(
        Func<BridgeSyncStatusSnapshot, BridgeSyncIndicator> indicatorFor,
        Func<BridgeException, string?>? lineFor = null)
    {
        _indicatorFor = indicatorFor;
        _lineFor = lineFor ?? BridgeDisplay.LocalizedLine;
    }

    public void Apply(BridgeSyncStatusSnapshot status)
    {
        CanReconnect = status.CanReconnect;
        ErrorText = status.Error is null ? null : _lineFor(status.Error);
        ErrorDetail = status.Error is null ? null : BridgeDisplay.FaultSummary(status.Error);
        Indicator = _indicatorFor(status);
        LastSyncTime = Indicator is BridgeSyncIndicator.Synced synced
            ? SyncIndicatorModel.FormatSyncTime(synced.LastSyncTime)
            : null;
        SyncReady = status.SyncReady;
        Blocked = status.Blocked;
        Changed?.Invoke();
    }

}
