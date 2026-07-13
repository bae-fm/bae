using System.Diagnostics;
using System.Reflection;
using uniffi.bae_bridge;

namespace Bae.Windows;

internal static class BaeDiagnostics
{
    private static readonly Assembly AppAssembly = Assembly.GetExecutingAssembly();

    internal static BaeLogger Logger { get; } = new("bae.windows");

    /// <summary>
    /// Build the Datadog telemetry config handed to library open. Telemetry is
    /// constructed inside the Rust core from it — there is no separate configure
    /// step. Local logging stays in <see cref="BaeLogger"/> (Trace).
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

    internal static void Flush(AppHandle handle)
    {
        var error = NativeBae.FlushDiagnostics(handle);
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
