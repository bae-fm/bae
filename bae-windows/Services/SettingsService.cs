using System;

namespace Bae.Windows;

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

    /// <summary>Wire the read through the open session's current handle.</summary>
    public static SettingsService FromSession(SessionStore session) => new()
    {
        GetSettings = () => session.WithCurrentHandle(NativeBae.GetSettings),
    };
}
