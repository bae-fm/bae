using System.Diagnostics;
using System.Reflection;
using uniffi.bae_bridge;

namespace Bae.Windows;

internal static class BaeDiagnostics
{
    private static readonly Assembly AppAssembly = Assembly.GetExecutingAssembly();

    internal static BaeLogger Logger { get; } = new("bae.windows");

    /// <summary>
    /// The process-lifetime telemetry sink, built once at startup by
    /// <see cref="Configure"/> and held for the whole app run. Keyring init and
    /// every library open require it, and it outlives any one library so
    /// exit-flush uses it directly.
    /// </summary>
    internal static BridgeDiagnostics Handle { get; private set; } = null!;

    /// <summary>
    /// Construct the telemetry sink and install the core's tracing subscriber.
    /// Call once at startup, before keyring init and any library open. Never
    /// throws: an enabled-config worker failure falls back to the disabled
    /// (no-op) sink.
    /// </summary>
    internal static BridgeDiagnostics Configure()
    {
        Handle = NativeBae.ConfigureDiagnostics(BridgeConfig());
        return Handle;
    }

    /// <summary>
    /// Build the Datadog telemetry config the sink is constructed from. Local
    /// logging stays in <see cref="BaeLogger"/> (Trace).
    /// </summary>
    internal static BridgeDiagnosticsConfig BridgeConfig()
    {
        var appVersion = AppAssembly.GetName().Version;
        if (appVersion is null)
        {
            Trace.TraceError("Assembly version is missing; diagnostics disabled");
            return new BridgeDiagnosticsConfig.Disabled();
        }

        return NativeBae.DiagnosticsConfig(
            AppMetadata.ConfiguredString("BaeDatadogSite"),
            AppMetadata.ConfiguredString("BaeDatadogClientToken"),
            "windows",
            "bae",
            AppMetadata.ConfiguredString("BaeEnvironment"),
            appVersion.ToString(),
            NativeBae.SupportsOAuthProviders() ? "bae" : "baeium",
            AppMetadata.ConfiguredString("BaeGitCommit"));
    }

    internal static void Flush()
    {
        var error = NativeBae.FlushDiagnostics(Handle);
        if (error is not null)
        {
            Trace.TraceError($"Failed to flush diagnostics: {error}");
        }
    }
}

internal sealed class BaeLogger
{
    private readonly string _target;

    internal BaeLogger(string target)
    {
        _target = target;
    }

    internal void Debug(string message, Exception? exception = null)
    {
        Write("debug", message, exception);
    }

    internal void Info(string message)
    {
        Write("info", message, null);
    }

    internal void Warning(string message, Exception? exception = null)
    {
        Write("warn", message, exception);
    }

    internal void Error(string message, Exception? exception = null)
    {
        Write("error", message, exception);
    }

    private void Write(
        string level,
        string message,
        Exception? exception)
    {
        TraceMessage(level, $"[{_target}] {Format(message, exception)}");
    }

    private static string Format(string message, Exception? exception) =>
        exception is null
            ? message
            : $"{message}: {exception.GetType().Name}: {exception.Message}";

    private static void TraceMessage(string level, string message)
    {
        switch (level)
        {
            case "debug":
            case "info":
                Trace.TraceInformation(message);
                break;
            case "warn":
                Trace.TraceWarning(message);
                break;
            case "error":
                Trace.TraceError(message);
                break;
        }
    }
}
