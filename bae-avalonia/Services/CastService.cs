using System;
using System.Collections.Generic;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Cast transport and discovery — the C# mirror of the macOS-only <c>Cast</c>
/// service (BaeKit has no cross-platform counterpart). Wraps the receiver
/// discovery, the switch to and from a device, and the device-list read the cast
/// picker shows. The cast <em>status</em> (which device, if any) is not read here;
/// it arrives on the castStatusChanged UI event, mirrored into <c>CastStore</c>.
/// Commands carry the session-swap currency the Windows session exposes; every
/// delegate defaults to a fail-loud stub and <see cref="FromSession"/> is the
/// production wiring.
/// </summary>
internal sealed class CastService
{
    /// <summary>The receivers found on the network, for the cast picker's list.</summary>
    public Func<(bool Current, IReadOnlyList<BridgeCastDevice> Devices)> GetCastDevices { get; init; }
        = () => throw new InvalidOperationException("CastService stub: GetCastDevices not wired");

    /// <summary>Begin browsing for receivers (the picker opened).</summary>
    public Func<bool> StartDiscovery { get; init; }
        = () => throw new InvalidOperationException("CastService stub: StartDiscovery not wired");

    /// <summary>Stop browsing (the picker closed).</summary>
    public Func<bool> StopDiscovery { get; init; }
        = () => throw new InvalidOperationException("CastService stub: StopDiscovery not wired");

    /// <summary>Switch playback to a receiver; the error line is the localized
    /// failure, or null on success.</summary>
    public Func<string, (bool Current, string? Error)> CastTo { get; init; }
        = _ => throw new InvalidOperationException("CastService stub: CastTo not wired");

    /// <summary>Stop casting and return playback to local output.</summary>
    public Func<bool> StopCasting { get; init; }
        = () => throw new InvalidOperationException("CastService stub: StopCasting not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static CastService FromSession(SessionStore session) => new()
    {
        GetCastDevices = () => session.WithCurrentHandle(NativeBae.GetCastDevices),
        StartDiscovery = () => session.WithCurrentHandle(NativeBae.StartCastDiscovery),
        StopDiscovery = () => session.WithCurrentHandle(NativeBae.StopCastDiscovery),
        CastTo = deviceId => session.WithCurrentHandle(handle => NativeBae.CastTo(handle, deviceId)),
        StopCasting = () => session.WithCurrentHandle(NativeBae.StopCasting),
    };
}
