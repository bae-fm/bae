using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// One release row in the storage manager, deserialized from the FFI's
/// <c>bae_storage</c> JSON.
/// </summary>
public sealed class StorageRow
{
    public string ReleaseId { get; set; } = string.Empty;
    public string AlbumTitle { get; set; } = string.Empty;
    public string Artist { get; set; } = string.Empty;
    public string? Format { get; set; }
    public string Size { get; set; } = string.Empty;
    public long FileCount { get; set; }
    public string State { get; set; } = string.Empty;
    public long PendingUploads { get; set; }

    /// <summary>The transitions this release allows now (pin/unpin/manage/unmanage),
    /// computed by the core (gated on cloud-home + pending uploads).</summary>
    public List<string> Actions { get; set; } = new();

    /// <summary>The list row, omitting absent fields.</summary>
    public string Summary
    {
        get
        {
            var format = string.IsNullOrEmpty(Format) ? string.Empty : $" · {Format}";
            var pending = PendingUploads > 0 ? $" · {PendingUploads} pending" : string.Empty;
            return $"{AlbumTitle} — {Artist}{format} · {FileCount} files · {Size} · {State}{pending}";
        }
    }
}
