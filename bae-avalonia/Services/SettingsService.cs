using System;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The library's settings (config) read. BaeKit has no config <em>service</em> —
/// macOS reads <c>appHandle.getConfig()</c> inside the config projection and
/// mirrors it into <c>ConfigStore</c> — so this is the Windows home for that read,
/// consumed by the settings mirror (<c>SettingsStore</c>, the ConfigStore analog).
/// The read carries the session-swap currency flag; a gone handle reads as not
/// current. Every delegate defaults to a fail-loud stub; <see cref="FromSession"/>
/// is the production wiring.
/// </summary>
internal sealed class SettingsService
{
    /// <summary>The current settings snapshot (display preferences, cloud, formats,
    /// concurrency). Not current when no handle is open.</summary>
    public Func<(bool Current, Settings Settings)> GetSettings { get; init; }
        = () => throw new InvalidOperationException("SettingsService stub: GetSettings not wired");

    /// <summary>The raw device-local config, for the storage sheet's transfer-
    /// concurrency pickers, which seed from the current simultaneous up/download
    /// counts. Not current when no handle is open.</summary>
    public Func<(bool Current, BridgeConfig Config)> GetConfig { get; init; }
        = () => throw new InvalidOperationException("SettingsService stub: GetConfig not wired");

    public Func<bool, (bool Current, string? Error)> SetAutomaticImportMetadataLookup { get; init; }
        = _ => throw new InvalidOperationException(
            "SettingsService stub: SetAutomaticImportMetadataLookup not wired");

    public Func<BridgeDefaultImportMetadataMode, (bool Current, string? Error)> SetDefaultImportMetadataMode { get; init; }
        = _ => throw new InvalidOperationException(
            "SettingsService stub: SetDefaultImportMetadataMode not wired");

    public Func<BridgeImportMetadataMode, (bool Current, string? Error)> SetLastImportMetadataMode { get; init; }
        = _ => throw new InvalidOperationException(
            "SettingsService stub: SetLastImportMetadataMode not wired");

    /// <summary>Wire the read through the open session's current handle.</summary>
    public static SettingsService FromSession(SessionStore session) => new()
    {
        GetSettings = () => session.WithCurrentHandle(NativeBae.GetSettings),
        GetConfig = () => session.WithCurrentHandle(NativeBae.GetConfig),
        SetAutomaticImportMetadataLookup = enabled =>
            session.WithCurrentHandle(handle =>
                NativeBae.SetAutomaticImportMetadataLookup(handle, enabled)),
        SetDefaultImportMetadataMode = mode =>
            session.WithCurrentHandle(handle =>
                NativeBae.SetDefaultImportMetadataMode(handle, mode)),
        SetLastImportMetadataMode = mode =>
            session.WithCurrentHandle(handle =>
                NativeBae.SetLastImportMetadataMode(handle, mode)),
    };
}
