using System.Text.Json.Serialization;

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
    public bool PauseBetweenSides { get; set; }

    /// <summary>Template rendering a single-track export's suggested filename.</summary>
    public string ExportFilenameTemplate { get; set; } = string.Empty;

    /// <summary>Which metadata tags a single-track export embeds.</summary>
    public ExportMetadata ExportMetadata { get; set; } = new();

    /// <summary>Configured export presets offered by release and track export.</summary>
    public List<ExportPreset> ExportPresets { get; set; } = new();

    /// <summary>Default selected option in the track export picker.</summary>
    public ExportSelection DefaultTrackExportSelection { get; set; } = ExportSelection.Original();

    /// <summary>Default selected option in the release export picker.</summary>
    public ExportSelection DefaultReleaseExportSelection { get; set; } = ExportSelection.Original();

    public bool McpEnabled { get; set; }
    public ushort McpPort { get; set; }
    public McpServerStatus McpStatus { get; set; } = new();
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
    public string McpStatusText => McpStatus.Status switch
    {
        "running" when !string.IsNullOrEmpty(McpStatus.Url) => Loc.Chrome(
            "settings.automation.status.running",
            "url",
            McpStatus.Url),
        "error" when McpStatus.Error is not null => McpStatus.Error.DisplayText,
        _ => Loc.Chrome("settings.automation.status.disabled"),
    };
}

/// <summary>
/// Which metadata tags a single-track export embeds — the seven booleans from
/// core's <c>ExportMetadata</c>. The JSON keys are snake_case
/// (<c>title</c>, <c>artist</c>, <c>album</c>, <c>year</c>, <c>track_number</c>,
/// <c>disc_number</c>, <c>cover_art</c>) via the shared snake_case naming policy,
/// so the PascalCase properties map without per-property attributes. All default
/// on, matching core's default.
/// </summary>
public sealed class ExportMetadata
{
    public bool Title { get; set; } = true;
    public bool Artist { get; set; } = true;
    public bool Album { get; set; } = true;
    public bool Year { get; set; } = true;
    public bool TrackNumber { get; set; } = true;
    public bool DiscNumber { get; set; } = true;
    public bool CoverArt { get; set; } = true;
}

public sealed class ExportPreset
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    public ExportPresetCodec Codec { get; set; } = new();
    public string Extension { get; set; } = string.Empty;
    public string FilenameTemplate { get; set; } = string.Empty;
    public ExportMetadata Metadata { get; set; } = new();
    public string PregapPlacement { get; set; } = "append_to_previous_except_htoa";
    public bool AppliesToTrack { get; set; }
    public bool AppliesToRelease { get; set; }

    [JsonIgnore]
    public string FileExtension => Extension.StartsWith(".", StringComparison.Ordinal) ? Extension : $".{Extension}";

    [JsonIgnore]
    public string TrackPickerLabel => Name;
}

public sealed class ExportPresetCodec
{
    public string Kind { get; set; } = string.Empty;
    public string BitDepth { get; set; } = "source";
    public uint BitrateKbps { get; set; }
}

public sealed class ExportSelection
{
    public string Kind { get; set; } = "original";
    public string? PresetId { get; set; }

    public static ExportSelection Original() => new() { Kind = "original" };

    public static ExportSelection Preset(string presetId) => new()
    {
        Kind = "preset",
        PresetId = presetId,
    };

    [JsonIgnore]
    public bool IsOriginal => Kind == "original";
}

public sealed class McpServerStatus
{
    public string Status { get; set; } = "disabled";
    public string? Url { get; set; }
    public McpServerStatusError? Error { get; set; }
}

public sealed class McpServerStatusError
{
    public string Kind { get; set; } = string.Empty;
    public string Detail { get; set; } = string.Empty;

    [JsonIgnore]
    public string Summary => Kind switch
    {
        "invalid_config" => Loc.Chrome("settings.automation.status.invalid_config"),
        "token_unavailable" => Loc.Chrome("settings.automation.status.token_unavailable"),
        "bind_failed" => Loc.Chrome("settings.automation.status.bind_failed"),
        "server_failed" => Loc.Chrome("settings.automation.status.server_failed"),
        _ => Loc.Chrome("settings.automation.status_unavailable"),
    };

    [JsonIgnore]
    public string DisplayText => string.IsNullOrEmpty(Detail) ? Summary : $"{Summary}: {Detail}";
}
