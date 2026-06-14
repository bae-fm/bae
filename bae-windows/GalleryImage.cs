namespace Bae.Windows;

/// <summary>One image in a release's gallery, from the FFI's <c>bae_gallery</c> JSON.</summary>
public sealed class GalleryImage
{
    public string Label { get; set; } = string.Empty;
    public string Path { get; set; } = string.Empty;
}
