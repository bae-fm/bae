namespace Bae.Windows;

/// <summary>
/// One of a release's local image files offered as a cover-art choice,
/// deserialized from the FFI's <c>bae_get_release_images</c> JSON. The picker
/// loads the thumbnail from <c>bae_gallery_bytes</c> of the release with a
/// <c>releaseFile</c> source built from <see cref="Id"/>, and passes
/// <see cref="Id"/> back to <c>bae_change_cover</c> when the user selects it.
/// </summary>
public sealed class ReleaseImage
{
    public string Id { get; set; } = string.Empty;
    public string OriginalFilename { get; set; } = string.Empty;
}
