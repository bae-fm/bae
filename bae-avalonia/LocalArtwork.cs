namespace Bae.Desktop;

/// <summary>
/// One image file in an import candidate's folder, offered as a cover choice.
/// The picker loads the thumbnail from <see cref="Path"/> and passes
/// <see cref="FileId"/> back as the <c>release_image</c> cover selection when
/// the user picks it.
/// </summary>
internal sealed class LocalArtwork
{
    /// <summary>Folder-relative path the import worker matches when selected.</summary>
    public string FileId { get; set; } = string.Empty;

    /// <summary>Absolute on-disk path the picker loads the thumbnail from.</summary>
    public string Path { get; set; } = string.Empty;
}
