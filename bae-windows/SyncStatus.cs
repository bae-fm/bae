namespace Bae.Windows;

/// <summary>Current cloud-sync status for the toolbar and sync banner.</summary>
public sealed class SyncStatus
{
    public DiagnosticError? Error { get; set; }
    public long? LastSyncTime { get; set; }
    public bool Syncing { get; set; }
    public bool SyncReady { get; set; }
}
