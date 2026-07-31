using System;
using System.Threading.Tasks;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// Automation (MCP) server settings and credentials, the C# mirror of the
/// macOS-only <c>Automation</c> closure-struct, used by the settings window's
/// Automation section. Token values are returned only for the copy/rotate actions
/// and never held in observable state. Commands run off the UI thread and carry
/// the session-swap currency the session exposes; the port comes pre-parsed from
/// the field. Every delegate defaults to a fail-loud stub; <see cref="FromSession"/>
/// is the production wiring.
/// </summary>
internal sealed class AutomationService
{
    /// <summary>Enable/disable the MCP server on a port; returns the error line, or
    /// null on success.</summary>
    public Func<bool, ushort, Task<(bool Current, string? Error)>> SetServerConfig { get; init; }
        = (_, _) => throw new InvalidOperationException("AutomationService stub: SetServerConfig not wired");

    /// <summary>The server's current run state, for the status line.</summary>
    public Func<Task<(bool Current, BridgeMcpServerStatus Status)>> ServerStatus { get; init; }
        = () => throw new InvalidOperationException("AutomationService stub: ServerStatus not wired");

    /// <summary>The current automation token, for a copy action; null when none is
    /// available.</summary>
    public Func<Task<(bool Current, string? Token)>> GetToken { get; init; }
        = () => throw new InvalidOperationException("AutomationService stub: GetToken not wired");

    /// <summary>Mint a fresh automation token (not yet stored); null on failure.</summary>
    public Func<Task<(bool Current, string? Token)>> GenerateToken { get; init; }
        = () => throw new InvalidOperationException("AutomationService stub: GenerateToken not wired");

    /// <summary>Store an automation token; returns the error line, or null on
    /// success.</summary>
    public Func<string, Task<(bool Current, string? Error)>> SetToken { get; init; }
        = _ => throw new InvalidOperationException("AutomationService stub: SetToken not wired");

    /// <summary>Wire every operation through the open session's current handle.</summary>
    public static AutomationService FromSession(SessionStore session) => new()
    {
        SetServerConfig = (enabled, port) =>
            session.RunForCurrentHandle(handle => NativeBae.SetMcpServerConfig(handle, enabled, port)),
        ServerStatus = () => session.RunForCurrentHandle(NativeBae.McpServerStatus),
        GetToken = () => session.RunForCurrentHandle(NativeBae.GetMcpToken),
        GenerateToken = () => session.RunForCurrentHandle(NativeBae.GenerateMcpToken),
        SetToken = token => session.RunForCurrentHandle(handle => NativeBae.SetMcpToken(handle, token)),
    };
}
