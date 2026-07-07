using System.Diagnostics;
using System.Reflection;

namespace Bae.Windows;

internal static class BaeDiagnostics
{
    private static readonly Assembly AppAssembly = Assembly.GetExecutingAssembly();

    internal static BaeLogger Logger { get; } = new("bae.windows");

    internal static void Configure()
    {
        var appVersion = AppAssembly.GetName().Version;
        if (appVersion is null)
        {
            Trace.TraceError("Assembly version is missing; diagnostics disabled");
            return;
        }

        var error = NativeBae.ConfigureDiagnostics(
            AppMetadata.ConfiguredString("BaeDatadogSite"),
            AppMetadata.ConfiguredString("BaeDatadogClientToken"),
            "windows",
            "bae",
            AppMetadata.ConfiguredString("BaeEnvironment"),
            appVersion.ToString(),
            NativeBae.SupportsOAuthProviders() ? "bae" : "baeium",
            AppMetadata.ConfiguredString("BaeGitCommit"));
        if (error is not null)
        {
            Trace.TraceError($"Failed to configure diagnostics: {error}");
        }
    }

    internal static void Flush()
    {
        var error = NativeBae.FlushDiagnostics();
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
        var formatted = Format(message, exception);
        TraceMessage(level, formatted);
        Log(level, formatted);
    }

    private void Log(
        string level,
        string message,
        IEnumerable<KeyValuePair<string, string>>? fields = null)
    {
        var error = NativeBae.DiagnosticsLog(
            level,
            _target,
            message,
            fields);
        if (error is not null)
        {
            Trace.TraceError($"Failed to send host diagnostic: {error}");
        }
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
