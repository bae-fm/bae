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
    /// on Windows and Linux alike, created if missing. Throws when the platform
    /// names no profile directory or the directory cannot be created, since no
    /// library can be found or created without it.
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
        BaeBridgeMethods.ConfigureDiagnostics(config, new CoreBugReporter());

    /// <summary>
    /// Build the Datadog telemetry config the sink is constructed from. Telemetry
    /// needs every build value — the Datadog site and client token, the
    /// environment, the app version and the git commit — so a build missing any
    /// sends nothing, and the log says which were missing, as Android's crash
    /// reporting does. Local logging stays in <see cref="BaeLogger"/>.
    /// </summary>
    internal static BridgeDiagnosticsConfig DiagnosticsConfig(
        string? datadogSite,
        string? clientToken,
        string source,
        string service,
        string? environment,
        string? appVersion,
        string edition,
        string? gitCommit,
        BaeLogger logger)
    {
        if (datadogSite is null || clientToken is null || environment is null || appVersion is null || gitCommit is null)
        {
            var missing = new (string Name, string? Value)[]
            {
                ("BaeDatadogSite", datadogSite),
                ("BaeDatadogClientToken", clientToken),
                ("BaeEnvironment", environment),
                ("the assembly version", appVersion),
                ("BaeGitCommit", gitCommit),
            }.Where(value => value.Value is null).Select(value => value.Name);
            logger.Warning(
                $"Telemetry disabled: not configured at build time (missing {string.Join(", ", missing)}). " +
                "Nothing will be sent.");
            return new BridgeDiagnosticsConfig.Disabled();
        }

        return new BridgeDiagnosticsConfig.Enabled(new BridgeDatadogDiagnosticsConfig(
            datadogSite,
            clientToken,
            source,
            new BridgeAppDiagnosticMetadata(service, environment, appVersion, edition, gitCommit)));
    }

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
            provider is BridgeCloudProvider.Dropbox
                or BridgeCloudProvider.OneDrive);

    /// <summary>The number of libraries registered on this device, read by the
    /// core from bae's directory on the host's runtime.</summary>
    internal static async Task<int> LibraryCount(BridgeHost host) =>
        (await host.ListLibraries()).Length;
}
