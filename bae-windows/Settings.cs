namespace Bae.Windows;

/// <summary>App settings from the FFI's <c>bae_settings</c> JSON.</summary>
public sealed class Settings
{
    public string LibraryName { get; set; } = string.Empty;
    public string LibraryId { get; set; } = string.Empty;

    /// <summary>"not_configured" / "valid" / "unvalidated" / "rejected".</summary>
    public string DiscogsStatus { get; set; } = "not_configured";

    /// <summary>
    /// Whether Discogs can be used as a metadata source — a stored key that
    /// isn't rejected. Core decides the policy; this is read, not re-derived.
    /// </summary>
    public bool DiscogsUsable { get; set; }

    /// <summary>Connected cloud provider's wire name, or null when not syncing.</summary>
    public string? SyncProvider { get; set; }
    public string? SyncAccount { get; set; }
    public bool SyncReady { get; set; }
    public bool HasCloudHome => SyncProvider is not null;

    /// <summary>One-line sync state for the settings header.</summary>
    public string SyncStatusText => SyncProvider is null
        ? "Sync: not connected"
        : $"Sync: {SyncProvider}{(string.IsNullOrEmpty(SyncAccount) ? string.Empty : $" ({SyncAccount})")} — {(SyncReady ? "ready" : "initializing")}";

    /// <summary>
    /// The persisted Discogs key is usable (stored and not rejected) — the
    /// dialog shows it as connected with Remove, not an editable input.
    /// "rejected" stored nothing, so it falls back to the input. Reads core's
    /// pre-computed usability flag rather than re-deriving it from the status.
    /// </summary>
    public bool DiscogsConfigured => DiscogsUsable;

    /// <summary>
    /// The stored key couldn't be reached at save time and is waiting to settle —
    /// the dialog offers a Re-check that re-validates it now.
    /// </summary>
    public bool DiscogsNeedsRecheck => DiscogsStatus == "unvalidated";

    /// <summary>
    /// One-line state for a stored key, shown when <see cref="DiscogsConfigured"/>.
    /// A rejected key stores nothing (so it isn't configured) and a missing key
    /// shows the editable input, so neither needs a label here.
    /// </summary>
    public string DiscogsStatusText => DiscogsStatus switch
    {
        "valid" => "connected",
        "unvalidated" => "saved — couldn't validate yet (offline). will retry.",
        _ => string.Empty,
    };
}
