using System;
using uniffi.bae_bridge;

namespace Bae.Windows;

/// <summary>
/// Image and file byte reads against the live library — the C# mirror of BaeKit's
/// <c>MediaPaths</c>. Wraps the narrow subset of the bridge that renders artwork
/// (a cover by id, a library image by ref, a gallery slot by its forwarded source)
/// and resolves the one remaining local file path (the DiscID external-file read),
/// so leaves take this instead of the session and <see cref="NativeBae"/>. Core
/// reads images locality-aware, fetching and decrypting from the cloud when they
/// aren't on disk here, so the UI never resolves a filesystem path itself. The
/// byte reads drop the session-swap currency flag — a gone handle reads as null
/// bytes, which the cover pipeline renders as a blank tile. Every delegate defaults
/// to a fail-loud stub; <see cref="FromSession"/> is the production wiring.
/// </summary>
internal sealed class MediaPathsService
{
    /// <summary>Bytes of a curated library image (a release cover or an artist
    /// portrait) by full image ref, or null when no such image exists.</summary>
    public Func<BridgeImageRef, byte[]?> FetchLibraryImageBytes { get; init; }
        = _ => throw new InvalidOperationException("MediaPathsService stub: FetchLibraryImageBytes not wired");

    /// <summary>Bytes of one of a release's image-strip slots (its cover or an
    /// image file), dispatched in core on the source.</summary>
    public Func<string, BridgeGallerySource, byte[]?> FetchReleaseImageBytes { get; init; }
        = (_, _) => throw new InvalidOperationException("MediaPathsService stub: FetchReleaseImageBytes not wired");

    /// <summary>Filesystem path for the user's own external file behind a library
    /// file, or null when there is none.</summary>
    public Func<string, (bool Current, (string? Path, string? Error) Result)> FilePath { get; init; }
        = _ => throw new InvalidOperationException("MediaPathsService stub: FilePath not wired");

    /// <summary>Bytes of provider art at a URL, for the import flow's cover search.</summary>
    public Func<string, byte[]?> FetchRemoteImageBytes { get; init; }
        = _ => throw new InvalidOperationException("MediaPathsService stub: FetchRemoteImageBytes not wired");

    /// <summary>Wire every read through the open session's current handle.</summary>
    public static MediaPathsService FromSession(SessionStore session) => new()
    {
        FetchLibraryImageBytes = image =>
            session.WithCurrentHandle(handle => NativeBae.LibraryImageBytes(handle, image)).Result,
        FetchReleaseImageBytes = (releaseId, source) =>
            session.WithCurrentHandle(handle => NativeBae.ReleaseImageBytes(handle, releaseId, source)).Result,
        FilePath = fileId =>
            session.WithCurrentHandle(handle => NativeBae.FilePath(handle, fileId)),
        FetchRemoteImageBytes = url =>
            session.WithCurrentHandle(handle => NativeBae.RemoteImageBytes(handle, url)).Result,
    };
}
