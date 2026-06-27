using System.Text.Json;

namespace Bae.Windows;

/// <summary>
/// One image in a release's gallery, from the FFI's <c>bae_gallery</c> JSON
/// (<c>{id, label, source}</c>). <see cref="Source"/> is the opaque byte-source
/// object the C# forwards verbatim to <c>bae_gallery_bytes</c> without inspecting
/// it; core dispatches the read on its <c>kind</c> (a cover or a release file).
/// </summary>
public sealed class GalleryImage
{
    public string Label { get; set; } = string.Empty;
    public JsonElement Source { get; set; }
}
