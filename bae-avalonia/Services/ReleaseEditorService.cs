using System;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The per-release actions reachable from album detail — set-primary, the cover
/// picker's reads and write, and (as the dialog family grows) edit / re-identify /
/// gallery / delete. The C# mirror of BaeKit's release-editor surface — BaeKit ReleaseEditor: one stored
/// delegate per operation, each wired through the open session so it carries the
/// session-swap currency contract. Delegates that carry the internal bridge cover
/// types are themselves internal (a public member exposing an internal type is
/// inconsistent accessibility). Every delegate defaults to a fail-loud stub;
/// <see cref="FromSession"/> is the production wiring.
/// </summary>
internal sealed class ReleaseEditorService
{
    /// <summary>Set the album's canonical release (the play/queue/cover default).</summary>
    public Func<string, string, Task<(bool Current, string? Error)>> SetPrimaryRelease { get; init; }
        = (_, _) => throw new InvalidOperationException("ReleaseEditorService stub: SetPrimaryRelease not wired");

    /// <summary>The release's own image files, the change-cover picker's first
    /// section. Synchronous (a local read) to open the dialog immediately.</summary>
    public Func<string, (bool Current, (BridgeFile[]? Images, string? Error) Result)> GetReleaseImages { get; init; }
        = _ => throw new InvalidOperationException("ReleaseEditorService stub: GetReleaseImages not wired");

    /// <summary>Remote cover candidates from MusicBrainz / Discogs — a network read,
    /// so async; the picker fills its remote section in when this lands.</summary>
    public Func<string, Task<(bool Current, (BridgeRemoteCover[]? Covers, string? Error) Result)>> FetchRemoteCovers { get; init; }
        = _ => throw new InvalidOperationException("ReleaseEditorService stub: FetchRemoteCovers not wired");

    /// <summary>Write the chosen cover (a release image or a downloaded remote one);
    /// the grid refreshes via the invalidation the change emits.</summary>
    public Func<string, string, BridgeCoverSelection, Task<(bool Current, string? Error)>> ChangeCover { get; init; }
        = (_, _, _) => throw new InvalidOperationException("ReleaseEditorService stub: ChangeCover not wired");

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
        GetReleaseImages = releaseId =>
            session.WithCurrentHandle(handle => NativeBae.GetReleaseImages(handle, releaseId)),
        FetchRemoteCovers = releaseId =>
            session.RunForCurrentHandle(handle => NativeBae.FetchRemoteCovers(handle, releaseId)),
        ChangeCover = (albumId, releaseId, selection) =>
            session.RunForCurrentHandle(handle => NativeBae.ChangeCover(handle, albumId, releaseId, selection)),
    };
}
