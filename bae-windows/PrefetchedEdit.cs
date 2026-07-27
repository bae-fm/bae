using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// The import confirmation seed. <see cref="Edit"/> seeds the metadata editor;
/// <see cref="RemoteCovers"/> (from the prefetched release detail) and
/// <see cref="LocalArtwork"/> (image files in the candidate's folder) are the
/// cover choices the confirm pane's picker offers before committing the import.
/// </summary>
internal sealed class PrefetchedEdit
{
    public BridgeRawReleaseEdit Edit { get; set; } = new(
        string.Empty,
        string.Empty,
        new BridgeRawPressingEdit(string.Empty, string.Empty, string.Empty, string.Empty, string.Empty, string.Empty),
        []);
    public List<BridgeRemoteCover> RemoteCovers { get; set; } = new();
    public List<LocalArtwork> LocalArtwork { get; set; } = new();

    /// <summary>
    /// What the pick claims and where its metadata came from, as bae-core
    /// derived it from the evidence that identified the candidate. Null for a
    /// skip-identify import, which claims nothing and has no source release.
    /// </summary>
    public BridgeClaimLine? Claim { get; set; }
}

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
