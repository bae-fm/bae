using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The import seed for one settled reading of a folder. <see cref="Edit"/> seeds
/// the release's own fields; <see cref="Mapping"/> is the table the tracklist is
/// committed from; <see cref="RemoteCovers"/> (from the prefetched release
/// detail) and <see cref="LocalArtwork"/> (image files in the candidate's
/// folder) are the cover choices the pane's picker offers before committing.
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
    /// derived it from the evidence that identified the candidate. Null for an
    /// Unknown import, which claims nothing and has no source release.
    /// </summary>
    public BridgeClaimLine? Claim { get; set; }

    /// <summary>
    /// The picked release's own pressing fields — what claiming this pressing
    /// exactly is a claim about. Null for an Unknown import, which claims
    /// nothing. Editing <see cref="Edit"/>'s pressing away from these is a
    /// different claim, and bae-core is what says so.
    /// </summary>
    public BridgeRawPressingEdit? ExactPressing { get; set; }

    /// <summary>
    /// The file↔release mapping this pick produces — every source unit the
    /// folder offers with the track committing makes of it, the editable row
    /// inside the row that produces it, and the tally over them. It is what the
    /// commit's tracklist is read back out of, so <see cref="Edit"/>'s own track
    /// rows are not the ones that get written.
    /// </summary>
    internal BridgeMappingTable Mapping { get; set; } = new([], [], Reconciliation: null);
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
