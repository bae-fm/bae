using System.Collections.Generic;

namespace Bae.Windows;

/// <summary>
/// The import confirmation seed, deserialized from the FFI's
/// <c>bae_prefetch_candidate_edit</c> JSON. <see cref="Edit"/> seeds the
/// metadata editor; <see cref="RemoteCovers"/> (from the prefetched release
/// detail) and <see cref="LocalArtwork"/> (image files in the candidate's
/// folder) are the cover choices the confirm pane's picker offers before
/// committing the import.
/// </summary>
public sealed class PrefetchedEdit
{
    public ReleaseEdit Edit { get; set; } = new();
    public List<RemoteCover> RemoteCovers { get; set; } = new();
    public List<LocalArtwork> LocalArtwork { get; set; } = new();
}

/// <summary>
/// One image file in an import candidate's folder, offered as a cover choice.
/// The picker loads the thumbnail from <see cref="Path"/> and passes
/// <see cref="FileId"/> back as the <c>release_image</c> cover selection when
/// the user picks it.
/// </summary>
public sealed class LocalArtwork
{
    /// <summary>Folder-relative path the import worker matches when selected.</summary>
    public string FileId { get; set; } = string.Empty;

    /// <summary>Absolute on-disk path the picker loads the thumbnail from.</summary>
    public string Path { get; set; } = string.Empty;
}
