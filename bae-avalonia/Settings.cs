using System.Text.Json.Serialization;
using uniffi.bae_bridge;

namespace Bae.Desktop;

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
    public bool PauseBetweenSides { get; set; }

    /// <summary>The method shown whenever Find online opens.</summary>
    internal BridgeDefaultFindOnlineMode DefaultFindOnlineMode { get; set; } =
        BridgeDefaultFindOnlineMode.Automatic;

    /// <summary>The source applied to a newly discovered candidate.</summary>
    internal BridgeDefaultImportMetadataSource DefaultImportMetadataSource { get; set; } =
        BridgeDefaultImportMetadataSource.FindOnline;

    /// <summary>Whether the seek bar's leading label counts down the time
    /// remaining instead of showing the time elapsed. A synced preference, so it
    /// follows the user to every device.</summary>
    public bool ShowRemainingTime { get; set; }

    /// <summary>Whether the library content column spans the full window width
    /// instead of the capped, centered column. A synced preference, so it follows
    /// the user to every device.</summary>
    public bool LibraryFullWidth { get; set; }

    /// <summary>Configured export presets offered by release and track export.
    /// The filename pattern is a per-preset property — there is no global one.</summary>
    public List<SavePreset> SavePresets { get; set; } = new();

    /// <summary>Id of the preset a track save defaults to (valid + track-applicable).</summary>
    public string DefaultTrackSavePreset { get; set; } = "flac";

    /// <summary>Id of the preset a release save defaults to (valid + release-applicable).</summary>
    public string DefaultReleaseSavePreset { get; set; } = "flac";

    /// <summary>Whether casting to a network receiver is available. Core enforces
    /// it — while off it browses no network and starts no session — so the UI
    /// reads this only to decide whether to show its cast control.</summary>
    public bool CastEnabled { get; set; }

    public bool McpEnabled { get; set; }
    public ushort McpPort { get; set; }
    internal BridgeMcpServerStatus McpStatus { get; set; } = new BridgeMcpServerStatus.Disabled();

    public bool SubsonicEnabled { get; set; }
    public ushort SubsonicPort { get; set; }
    public string SubsonicUsername { get; set; } = string.Empty;

    /// <summary>The IP the server binds. "127.0.0.1" keeps it on this machine;
    /// "0.0.0.0" opens it to other devices on the network. The UI presents this
    /// as a network-access toggle, not a raw address field.</summary>
    public string SubsonicBindAddress { get; set; } = "127.0.0.1";
    internal BridgeSubsonicServerStatus SubsonicStatus { get; set; } = new BridgeSubsonicServerStatus.Disabled();
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
            var key = BridgeDisplay.CloudProviderLabelKey(SyncProvider);
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
    public string SyncStatusText(bool? syncReady)
    {
        if (SyncProvider is null)
        {
            return Loc.Chrome("settings.sync.not_connected");
        }
        var account = string.IsNullOrEmpty(SyncAccount) ? string.Empty : $" ({SyncAccount})";
        var state = syncReady == true
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

    [JsonIgnore]
    public string SubsonicStatusText => SubsonicStatusTextFor(SubsonicStatus);

    internal static string SubsonicStatusTextFor(BridgeSubsonicServerStatus status) => status switch
    {
        BridgeSubsonicServerStatus.Running running when !string.IsNullOrEmpty(running.Url) => Loc.Chrome(
            "settings.subsonic.status.running",
            "url",
            running.Url),
        BridgeSubsonicServerStatus.Error error => SubsonicErrorDisplayText(error.ErrorValue),
        _ => Loc.Chrome("settings.subsonic.status.disabled"),
    };

    private static string SubsonicErrorDisplayText(BridgeSubsonicServerError error)
    {
        var (summary, detail) = error switch
        {
            BridgeSubsonicServerError.InvalidConfig invalid => (Loc.Chrome("settings.subsonic.status.invalid_config"), invalid.Detail),
            BridgeSubsonicServerError.CredentialUnavailable credential => (Loc.Chrome("settings.subsonic.status.credential_unavailable"), credential.Detail),
            BridgeSubsonicServerError.BindFailed bind => (Loc.Chrome("settings.subsonic.status.bind_failed"), bind.Detail),
            BridgeSubsonicServerError.ServerFailed server => (Loc.Chrome("settings.subsonic.status.server_failed"), server.Detail),
            _ => (Loc.Chrome("settings.subsonic.status_unavailable"), string.Empty),
        };
        return string.IsNullOrEmpty(detail) ? summary : $"{summary}: {detail}";
    }
}

public sealed class SavePreset
{
    public string Id { get; set; } = string.Empty;
    public string Name { get; set; } = string.Empty;
    internal BridgeSaveCodec Codec { get; set; } = new BridgeSaveCodec.Flac(BridgeSaveBitDepth.Source);
    public string Extension { get; set; } = string.Empty;
    internal List<BridgeSaveFilenameToken> FilenameTokens { get; set; } = new();
    internal BridgeSavePregapPlacement PregapPlacement { get; set; } = BridgeSavePregapPlacement.AppendToPreviousExceptHtoa;
    public bool AppliesToTrack { get; set; }
    public bool AppliesToRelease { get; set; }

    /// <summary>Whether saved files embed the release's cover art.</summary>
    public bool EmbedCover { get; set; } = true;

    [JsonIgnore]
    public string FileExtension => Extension.StartsWith(".", StringComparison.Ordinal) ? Extension : $".{Extension}";

    [JsonIgnore]
    public string TrackPickerLabel => Name;
}
