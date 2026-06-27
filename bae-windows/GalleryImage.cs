namespace Bae.Windows;

/// <summary>
/// One image in a release's gallery, from the FFI's <c>bae_gallery</c> JSON
/// (<c>{id, label, cover_version}</c>). Read by id: a non-null
/// <see cref="CoverVersion"/> marks the cover slot (fetch via
/// <c>bae_image_bytes</c> with the id), null marks a release-file image (fetch via
/// <c>bae_gallery_image_bytes</c> with the release id and the id).
/// </summary>
public sealed class GalleryImage
{
    public string Id { get; set; } = string.Empty;
    public string Label { get; set; } = string.Empty;
    public string? CoverVersion { get; set; }
}
