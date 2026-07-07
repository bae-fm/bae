using System;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Mirror of core's sync-status snapshot for the toolbar indicator and the sync
// banner. Refresh reads the snapshot from the current handle; the window renders
// the indicator and banner from these fields on Changed.
internal sealed class SyncStatusStore
{
    private readonly SessionStore _session;

    // The localized sync-error line, or null when sync is healthy.
    public string? ErrorText { get; private set; }
    public bool Syncing { get; private set; }
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
        Syncing = status.Syncing;
        LastSyncTime = SyncIndicatorModel.FormatSyncTime(status.LastSyncTime);
        Changed?.Invoke();
    }

    public void Reset()
    {
        ErrorText = null;
        Syncing = false;
        LastSyncTime = null;
        Changed?.Invoke();
    }
}
