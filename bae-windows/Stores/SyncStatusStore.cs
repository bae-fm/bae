using System;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Mirror of core's sync-status snapshot for the toolbar indicator and the sync
// banner. Refresh reads the snapshot from the current handle; the window renders
// the indicator and banner from these fields on Changed.
internal sealed class SyncStatusStore
{
    private readonly SessionStore _session;

    // The localized sync-error line, or null when sync is healthy. Kept for the
    // reconnect banner; the toolbar badge is driven by Indicator.
    public string? ErrorText { get; private set; }

    // The badge state, decided by core (error > syncing > synced > idle). The UI
    // maps it to a label and colour and never re-derives which state wins.
    public BridgeSyncIndicator Indicator { get; private set; } = new BridgeSyncIndicator.Idle();

    // The formatted last-sync time, set only when Indicator is Synced — so a stale
    // timestamp can never accompany a stopped loop.
    public string? LastSyncTime { get; private set; }

    public event Action? Changed;

    public SyncStatusStore(SessionStore session)
    {
        _session = session;
    }

    public void Refresh()
    {
        var (current, result) = _session.WithCurrentHandle(NativeBae.SyncStatus);
        if (!current)
        {
            return;
        }
        if (result.Error is not null)
        {
            BaeDiagnostics.Logger.Error($"Failed to read sync status snapshot: {result.Error}");
            Reset();
            return;
        }

        var status = result.Status;
        if (status is null)
        {
            BaeDiagnostics.Logger.Error("Failed to read sync status snapshot.");
            Reset();
            return;
        }

        ErrorText = status.Error is null ? null : BridgeDisplay.LocalizedLine(status.Error);
        Indicator = BaeBridgeMethods.BridgeSyncIndicator(status);
        LastSyncTime = Indicator is BridgeSyncIndicator.Synced synced
            ? SyncIndicatorModel.FormatSyncTime(synced.LastSyncTime)
            : null;
        Changed?.Invoke();
    }

    public void Reset()
    {
        ErrorText = null;
        Indicator = new BridgeSyncIndicator.Idle();
        LastSyncTime = null;
        Changed?.Invoke();
    }
}
