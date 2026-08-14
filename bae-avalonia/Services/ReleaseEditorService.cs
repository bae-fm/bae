using System;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The per-release actions reachable from album detail — set-primary, the cover
/// picker's reads and write, the metadata edit round-trip, and (as the dialog
/// family grows) re-identify. The C# mirror of BaeKit's <c>ReleaseEditor</c>
/// closure-struct: one stored delegate per operation, each wired through the open
/// session so it carries the session-swap currency contract. Delegates whose
/// signatures carry an internal bridge type stay internal (a public member exposing
/// an internal type is inconsistent accessibility). Every delegate defaults to a
/// fail-loud stub; <see cref="FromSession"/> is the production wiring.
/// </summary>
internal sealed class ReleaseEditorService
{
    /// <summary>Set the album's canonical release (the play/queue/cover default).</summary>
    public Func<string, string, Task<(bool Current, string? Error)>> SetPrimaryRelease { get; init; }
        = (_, _) => throw new InvalidOperationException("ReleaseEditorService stub: SetPrimaryRelease not wired");

    /// <summary>Remote cover candidates from MusicBrainz / Discogs — a network read,
    /// so async; the picker fills its remote section in when this lands.</summary>
    public Func<string, Task<(bool Current, (BridgeRemoteCover[]? Covers, string? Error) Result)>> FetchRemoteCovers { get; init; }
        = _ => throw new InvalidOperationException("ReleaseEditorService stub: FetchRemoteCovers not wired");

    /// <summary>Write the chosen cover (a release image or a downloaded remote one).
    /// Open album subscriptions deliver the updated cover version.</summary>
    public Func<string, BridgeCoverSelection, Task<(bool Current, string? Error)>> ChangeCover { get; init; }
        = (_, _) => throw new InvalidOperationException("ReleaseEditorService stub: ChangeCover not wired");

    /// <summary>The editable metadata seed for a release — album/pressing fields and
    /// the per-track table — the edit form populates from.</summary>
    public Func<string, Task<(bool Current, (BridgeRawReleaseEdit? Edit, string? Error) Result)>> ReleaseEditSeed { get; init; }
        = _ => throw new InvalidOperationException("ReleaseEditorService stub: ReleaseEditSeed not wired");

    /// <summary>Commit the edited metadata. Shaping and validation happen in core;
    /// a validation error keeps the dialog open with the reason.</summary>
    public Func<string, BridgeRawReleaseEdit, Task<(bool Current, string? Error)>> ApplyReleaseEdit { get; init; }
        = (_, _) => throw new InvalidOperationException("ReleaseEditorService stub: ApplyReleaseEdit not wired");

    /// <summary>Discard in-progress edits and re-seed the form from the release's
    /// stored metadata source (its original identity), without writing. Async — it
    /// re-projects from the source.</summary>
    public Func<string, Task<(bool Current, (BridgeRawReleaseEdit? Edit, string? Error) Result)>> ResetMetadataToSource { get; init; }
        = _ => throw new InvalidOperationException("ReleaseEditorService stub: ResetMetadataToSource not wired");

    /// <summary>Commit a re-identify: point the release at a chosen source pressing
    /// (exact or metadata-only) or clear its identity. Open subscriptions deliver
    /// the updated release.</summary>
    public Func<string, BridgeIdentityChoice, Task<(bool Current, string? Error)>> ReidentifyRelease { get; init; }
        = (_, _) => throw new InvalidOperationException("ReleaseEditorService stub: ReidentifyRelease not wired");

    /// <summary>Reseed the release's metadata from its (just re-pointed) source,
    /// overwriting prior edits by design. Offered after a source-backed re-identify.</summary>
    public Func<string, Task<(bool Current, string? Error)>> RefreshMetadataFromSource { get; init; }
        = _ => throw new InvalidOperationException("ReleaseEditorService stub: RefreshMetadataFromSource not wired");

    /// <summary>The thumbnail URL for a remote cover candidate — a pure transform of
    /// the bridge value, kept in-boundary so views never touch NativeBae.</summary>
    public static string RemoteCoverThumbnailUrl(BridgeRemoteCover cover) =>
        NativeBae.RemoteCoverThumbnailUrl(cover);

    /// <summary>The cover-selection payload for a remote candidate.</summary>
    public static BridgeCoverSelection RemoteCoverSelection(BridgeRemoteCover cover) =>
        NativeBae.RemoteCoverSelection(cover);

    public static ReleaseEditorService FromSession(SessionStore session) => new()
    {
        SetPrimaryRelease = (albumId, releaseId) =>
            session.RunForCurrentHandle(handle => NativeBae.SetPrimaryRelease(handle, albumId, releaseId)),
        FetchRemoteCovers = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.FetchRemoteCovers(handle, releaseId)),
        ChangeCover = (releaseId, selection) =>
            session.RunForCurrentHandle(handle => NativeBae.ChangeCover(handle, releaseId, selection)),
        ReleaseEditSeed = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.ReleaseEditSeed(handle, releaseId)),
        ApplyReleaseEdit = (releaseId, edit) =>
            session.RunForCurrentHandle(handle => NativeBae.ApplyReleaseEdit(handle, releaseId, edit)),
        ResetMetadataToSource = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.ResetMetadataToSource(handle, releaseId)),
        ReidentifyRelease = (releaseId, choice) =>
            session.RunForCurrentHandle(handle => NativeBae.ReidentifyRelease(handle, releaseId, choice)),
        RefreshMetadataFromSource = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.RefreshMetadataFromSource(handle, releaseId)),
    };
}
