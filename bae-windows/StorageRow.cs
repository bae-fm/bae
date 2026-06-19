using System.Collections.Generic;
using System.Text.Json.Serialization;

namespace Bae.Windows;

/// <summary>
/// One release row in the storage manager, deserialized from the FFI's
/// <c>bae_storage</c> JSON. The locale never crosses the bridge: the size is a
/// raw byte count and the state is a wire tag, both rendered for the locale here.
/// </summary>
public sealed class StorageRow
{
    public string ReleaseId { get; set; } = string.Empty;
    public string AlbumTitle { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;
    public string? Format { get; set; }

    /// <summary>Raw total size in bytes; formatted for the locale by
    /// <see cref="SizeLabel"/>.</summary>
    public long TotalSize { get; set; }

    public long FileCount { get; set; }

    /// <summary>Storage-state wire tag: "unmanaged" / "pinned" / "cloud_only".</summary>
    public string State { get; set; } = string.Empty;

    public long PendingUploads { get; set; }

    /// <summary>The transitions this release allows now (pin/unpin/manage/unmanage),
    /// computed by the core (gated on cloud-home + pending uploads).</summary>
    public List<string> Actions { get; set; } = new();

    /// <summary>The total size formatted for the locale, e.g. "412 MB".</summary>
    [JsonIgnore]
    public string SizeLabel => Loc.Bytes(TotalSize);

    /// <summary>
    /// The localized storage-state label. macOS does not yet route this typed
    /// state through the shared catalog (it hardcodes the words in Swift), so
    /// it's app chrome here — keyed by the wire tag in the app's own Resources
    /// table — rather than an invented <c>core.*</c> key macOS doesn't have.
    /// </summary>
    [JsonIgnore]
    public string StateLabel => Loc.Chrome($"storage.state.{State}");

    /// <summary>The list row, omitting absent fields.</summary>
    [JsonIgnore]
    public string Summary
    {
        get
        {
            var format = string.IsNullOrEmpty(Format) ? string.Empty : $" · {Format}";
            // File count is app chrome (pluralized via the MF1 value in Resources);
            // pending uploads reuses the shared core.queue.uploading message.
            var files = Loc.Chrome("storage.files", "count", FileCount);
            var pending = PendingUploads > 0
                ? $" · {Loc.Core("core.queue.uploading", "count", PendingUploads)}"
                : string.Empty;
            return $"{AlbumTitle} — {Artist}{format} · {files} · {SizeLabel} · {StateLabel}{pending}";
        }
    }
}
