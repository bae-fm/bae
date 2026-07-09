using System.Text.Json.Serialization;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>App settings from the generated bridge config.</summary>
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
    public bool PauseBetweenSides { get; set; }

    /// <summary>Where release exports write: a fixed folder, or prompt each time.</summary>
    internal BridgeExportLocation ExportLocation { get; set; } = new BridgeExportLocation.AskEachTime();

    /// <summary>Template rendering a single-track export's suggested filename.</summary>
    public string ExportFilenameTemplate { get; set; } = string.Empty;

    /// <summary>Configured export presets offered by release and track export.</summary>
    public List<ExportPreset> ExportPresets { get; set; } = new();

    /// <summary>Default selected option in the track export picker.</summary>
    internal BridgeExportSelection DefaultTrackExportSelection { get; set; } = new BridgeExportSelection.Original();

    /// <summary>Default selected option in the release export picker.</summary>
    internal BridgeExportSelection DefaultReleaseExportSelection { get; set; } = new BridgeExportSelection.Original();

    public bool McpEnabled { get; set; }
    public ushort McpPort { get; set; }
    internal BridgeMcpServerStatus McpStatus { get; set; } = new BridgeMcpServerStatus.Disabled();
    public bool HasCloudHome => SyncProvider is not null;

    /// <summary>
    /// The connected provider's display name. The locale never crosses the
    /// bridge: the FFI maps the wire tag to a catalog key (S3 → "S3-compatible",
    /// local-only → "Local only") or null for the brand-name providers the UI
    /// passes through verbatim (Google Drive, Dropbox, OneDrive, iCloud). Mirrors
    /// macOS's <c>localizedCloudProviderName</c>.
    /// </summary>
    [JsonIgnore]
    public string ProviderLabel
    {
        get
        {
            var key = NativeBae.CloudProviderLabelKey(SyncProvider);
            if (key is not null)
            {
                return Loc.Core(key);
            }
            // No key → a brand name the UI shows verbatim; map the wire tag to
            // its proper-noun chrome label.
            return SyncProvider switch
            {
                "google_drive" => Loc.Chrome("cloud.provider.google_drive"),
                "dropbox" => Loc.Chrome("cloud.provider.dropbox"),
                "onedrive" => Loc.Chrome("cloud.provider.onedrive"),
                "cloudkit" => Loc.Chrome("cloud.provider.icloud"),
                _ => SyncProvider ?? string.Empty,
            };
        }
    }

    /// <summary>One-line sync state for the settings header.</summary>
    [JsonIgnore]
    public string SyncStatusText
    {
        get
        {
            if (SyncProvider is null)
            {
                return Loc.Chrome("settings.sync.not_connected");
            }
            var account = string.IsNullOrEmpty(SyncAccount) ? string.Empty : $" ({SyncAccount})";
            var state = SyncReady
                ? Loc.Chrome("settings.sync.ready")
                : Loc.Chrome("settings.sync.initializing");
            return Loc.Chrome(
                "settings.sync.status",
                new System.Collections.Generic.Dictionary<string, object?>
                {
                    ["provider"] = ProviderLabel,
                    ["account"] = account,
                    ["state"] = state,
                });
        }
    }

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
    [JsonIgnore]
    public string DiscogsStatusText => DiscogsStatus switch
    {
        "valid" => Loc.Chrome("settings.discogs.connected"),
        "unvalidated" => Loc.Chrome("settings.discogs.unvalidated"),
        _ => string.Empty,
    };

    [JsonIgnore]
    public string McpStatusText => McpStatusTextFor(McpStatus);

    internal static string McpStatusTextFor(BridgeMcpServerStatus status) => status switch
    {
        BridgeMcpServerStatus.Running running when !string.IsNullOrEmpty(running.Url) => Loc.Chrome(
            "settings.automation.status.running",
            "url",
            running.Url),
        BridgeMcpServerStatus.Error error => McpErrorDisplayText(error.ErrorValue),
        _ => Loc.Chrome("settings.automation.status.disabled"),
    };

    private static string McpErrorDisplayText(BridgeMcpServerError error)
    {
        var (summary, detail) = error switch
        {
            BridgeMcpServerError.InvalidConfig invalid => (Loc.Chrome("settings.automation.status.invalid_config"), invalid.Detail),
            BridgeMcpServerError.TokenUnavailable token => (Loc.Chrome("settings.automation.status.token_unavailable"), token.Detail),
            BridgeMcpServerError.BindFailed bind => (Loc.Chrome("settings.automation.status.bind_failed"), bind.Detail),
            BridgeMcpServerError.ServerFailed server => (Loc.Chrome("settings.automation.status.server_failed"), server.Detail),
            _ => (Loc.Chrome("settings.automation.status_unavailable"), string.Empty),
        };
        return string.IsNullOrEmpty(detail) ? summary : $"{summary}: {detail}";
    }
}

public sealed class ExportPreset
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    internal BridgeExportPresetCodec Codec { get; set; } = new BridgeExportPresetCodec.Flac(BridgeExportBitDepth.Source);
    public string Extension { get; set; } = string.Empty;
    public string FilenameTemplate { get; set; } = string.Empty;
    internal BridgeExportPregapPlacement PregapPlacement { get; set; } = BridgeExportPregapPlacement.AppendToPreviousExceptHtoa;
    public bool AppliesToTrack { get; set; }
    public bool AppliesToRelease { get; set; }

    [JsonIgnore]
    public string FileExtension => Extension.StartsWith(".", StringComparison.Ordinal) ? Extension : $".{Extension}";

    [JsonIgnore]
    public string TrackPickerLabel => Name;
}
