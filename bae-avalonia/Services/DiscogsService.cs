using System;
using System.Threading.Tasks;

namespace Bae.Desktop;

/// <summary>
/// Discogs API-key persistence, the C# mirror of the macOS-only <c>Discogs</c>
/// closure-struct, used by the settings window's Discogs section. <c>Save</c>
/// validates the key against Discogs before storing and reports the outcome so a
/// rejected draft survives; <c>Revalidate</c> re-checks a key stored while offline.
/// The network operations run off the UI thread and carry the session-swap
/// currency the session exposes. Every delegate defaults to a fail-loud stub;
/// <see cref="FromSession"/> is the production wiring.
/// </summary>
internal sealed class DiscogsService
{
    /// <summary>Validate and store a Discogs key, returning the outcome tag
    /// (<c>valid</c> / <c>unvalidated</c> / <c>rejected</c>), or null when the write
    /// itself failed.</summary>
    public Func<string, Task<(bool Current, string? Outcome)>> SaveToken { get; init; }
        = _ => throw new InvalidOperationException("DiscogsService stub: SaveToken not wired");

    /// <summary>Re-check a stored key against Discogs (a no-op unless the stored key
    /// is unvalidated); returns the error line, or null on success.</summary>
    public Func<Task<(bool Current, string? Error)>> RevalidateToken { get; init; }
        = () => throw new InvalidOperationException("DiscogsService stub: RevalidateToken not wired");

    /// <summary>Forget the stored key; returns the error line, or null on success.</summary>
    public Func<Task<(bool Current, string? Error)>> RemoveToken { get; init; }
        = () => throw new InvalidOperationException("DiscogsService stub: RemoveToken not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static DiscogsService FromSession(SessionStore session) => new()
    {
        SaveToken = token => session.RunForCurrentHandle(handle => NativeBae.SaveDiscogsToken(handle, token)),
        RevalidateToken = () => session.RunForCurrentHandle(NativeBae.RevalidateDiscogsToken),
        RemoveToken = () => session.RunForCurrentHandle(NativeBae.DeleteDiscogsToken),
    };
}
