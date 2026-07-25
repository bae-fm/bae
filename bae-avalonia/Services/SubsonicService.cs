using System;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Subsonic server settings and credentials, the C# mirror of the macOS-only
/// <c>SubsonicServer</c> closure-struct, used by the settings window's Subsonic
/// section. The password is write-only from here (committed to the keyring) and
/// never read back into state. Commands run off the UI thread and carry the
/// session-swap currency the session exposes; the port comes pre-parsed from the
/// field. Every delegate defaults to a fail-loud stub; <see cref="FromSession"/>
/// is the production wiring.
/// </summary>
internal sealed class SubsonicService
{
    /// <summary>Enable/disable the Subsonic server on a port, with the username and
    /// bind address; returns the error line, or null on success.</summary>
    public Func<bool, ushort, string, string, Task<(bool Current, string? Error)>> SetServerConfig { get; init; }
        = (_, _, _, _) => throw new InvalidOperationException("SubsonicService stub: SetServerConfig not wired");

    /// <summary>The server's current run state, for the status line.</summary>
    public Func<Task<(bool Current, BridgeSubsonicServerStatus Status)>> ServerStatus { get; init; }
        = () => throw new InvalidOperationException("SubsonicService stub: ServerStatus not wired");

    /// <summary>Store the Subsonic password in the keyring; returns the error line,
    /// or null on success.</summary>
    public Func<string, Task<(bool Current, string? Error)>> SetPassword { get; init; }
        = _ => throw new InvalidOperationException("SubsonicService stub: SetPassword not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static SubsonicService FromSession(SessionStore session) => new()
    {
        SetServerConfig = (enabled, port, username, bindAddress) =>
            session.RunForCurrentHandle(handle => NativeBae.SetSubsonicServerConfig(handle, enabled, port, username, bindAddress)),
        ServerStatus = () => session.RunForCurrentHandle(NativeBae.SubsonicServerStatus),
        SetPassword = password => session.RunForCurrentHandle(handle => NativeBae.SetSubsonicPassword(handle, password)),
    };
}
