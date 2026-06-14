namespace Bae.Windows;

/// <summary>
/// Whether a candidate release is already in the library, deserialized from the
/// FFI's <c>bae_check_release_in_library</c> JSON. The import confirmation shows
/// a banner when <see cref="ReleaseInLibrary"/> is set; when <see cref="AlbumId"/>
/// is present the banner links to that album.
/// </summary>
public sealed class LibraryStatus
{
    /// <summary>The exact pressing (same source + release id) is in the library.</summary>
    public bool ReleaseInLibrary { get; set; }

    /// <summary>The library album to open, when one matched.</summary>
    public string? AlbumId { get; set; }
}
