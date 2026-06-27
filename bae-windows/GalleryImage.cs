namespace Bae.Windows;

/// <summary>
/// One image in a release's gallery, from the FFI's <c>bae_gallery</c> JSON
/// (<c>{id, label, source}</c>). <see cref="Source"/> names which byte source the
/// slot is read from — a cover or a release file.
/// </summary>
public sealed class GalleryImage
{
    public string Id { get; set; } = string.Empty;
    public string Label { get; set; } = string.Empty;
    public GallerySource Source { get; set; } = new();
}

/// <summary>
/// Which byte source a gallery slot is read from. Mirrors the FFI's
/// <c>FfiGallerySource</c>, a <c>kind</c>-tagged object: <see cref="Kind"/> is
/// <c>"cover"</c> — <see cref="Cover"/> holds the cover's image reference, fetched
/// via <c>bae_image_bytes</c> with its id — or <c>"releaseFile"</c>, fetched via
/// <c>bae_gallery_image_bytes</c> with the release id and the item's id.
/// </summary>
public sealed class GallerySource
{
    public string Kind { get; set; } = string.Empty;
    public ImageRef? Cover { get; set; }
}
