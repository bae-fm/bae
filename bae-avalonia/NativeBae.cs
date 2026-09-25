using System.Linq;
using uniffi.bae_bridge;

namespace Bae.Desktop;

/// <summary>
/// The app's calls into the generated bridge bindings: the process-lifetime
/// telemetry sink and host, and the one library read the skeleton window shows.
/// </summary>
internal static class NativeBae
{
    /// <summary>
    /// bae's directory under the user's profile directory — the home directory
    /// on Windows and Linux alike. Throws when the platform names no profile
    /// directory, since no library can be found or created without one.
    /// </summary>
    internal static BridgeAppDir UserAppDir()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (string.IsNullOrEmpty(home))
        {
            throw new InvalidOperationException("The user profile directory is unknown, so bae's directory has no location.");
        }

        return new BridgeAppDir(home);
    }

    /// <summary>
    /// Construct the telemetry sink and install the core's tracing subscriber.
    /// Infallible: the core falls back to the no-op sink (with a local error
    /// log) rather than let telemetry setup block a launch.
    /// </summary>
    internal static BridgeDiagnostics ConfigureDiagnostics(BridgeDiagnosticsConfig config) =>
        BaeBridgeMethods.ConfigureDiagnostics(config);

    /// <summary>
    /// Build the Datadog telemetry config the sink is constructed from. Local
    /// logging stays in <see cref="BaeLogger"/>.
    /// </summary>
    internal static BridgeDiagnosticsConfig DiagnosticsConfig(
        string? datadogSite,
        string? clientToken,
        string source,
        string service,
        string? environment,
        string appVersion,
        string edition,
        string? gitCommit) =>
        datadogSite is not null && clientToken is not null
            ? new BridgeDiagnosticsConfig.Enabled(new BridgeDatadogDiagnosticsConfig(
                datadogSite,
                clientToken,
                source,
                new BridgeAppDiagnosticMetadata(
                    service,
                    environment ?? string.Empty,
                    appVersion,
                    edition,
                    gitCommit ?? string.Empty)))
            : new BridgeDiagnosticsConfig.Disabled();

    /// <summary>
    /// Build the process-lifetime host registrations around the telemetry sink
    /// and bae's directory. Throws when the OS refuses the host runtime's worker
    /// threads.
    /// </summary>
    internal static BridgeHost CreateHost(BridgeDiagnostics diagnostics, BridgeAppDir appDir) =>
        new BridgeHost(diagnostics, appDir);

    /// <summary>Flush buffered telemetry through the host's runtime. Returns the
    /// failure's message, or null when the flush completed.</summary>
    internal static async Task<string?> FlushDiagnostics(BridgeHost host)
    {
        try
        {
            await host.FlushDiagnostics();
            return null;
        }
        catch (BridgeException.Cancelled)
        {
            return null;
        }
        catch (BridgeException exception)
        {
            return exception.Message;
        }
    }

    /// <summary>Whether this build's native library supports any OAuth cloud
    /// provider — the full (bae) edition, as opposed to the S3-only baeium.</summary>
    internal static bool SupportsOAuthProviders() =>
        BaeBridgeMethods.AvailableCloudProviders().Any(provider =>
            provider is BridgeCloudProvider.GoogleDrive
                or BridgeCloudProvider.Dropbox
                or BridgeCloudProvider.OneDrive);

    /// <summary>The number of libraries registered on this device, read by the
    /// core from bae's directory.</summary>
    internal static int LibraryCount(BridgeHost host) => host.DiscoverLibraries().Length;
}
