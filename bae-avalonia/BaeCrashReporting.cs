using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using Sentry;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static class BaeCrashReporting
{
    private static readonly Assembly AppAssembly = Assembly.GetExecutingAssembly();
    private static IDisposable? _managedSdk;

    internal static void Configure()
    {
        if (_managedSdk is not null)
        {
            return;
        }

        var config = Config();
        if (config is null)
        {
            return;
        }

        _managedSdk = SentrySdk.Init(options =>
        {
            options.Dsn = config.Dsn;
            options.Environment = config.Environment;
            options.Release = config.Release;
            options.SendDefaultPii = false;
        });
        SentrySdk.ConfigureScope(scope =>
        {
            scope.SetTag("edition", config.Edition);
            scope.SetTag("git_commit", config.GitCommit);
        });
        ConfigureNative(config);
    }

    // Forward a managed exception caught by an unhandled-exception hook to Sentry.
    // A no-op when the SDK was never initialized (an unconfigured edition, or a
    // crash before Configure ran), which SentrySdk handles without throwing.
    internal static void CaptureException(Exception exception)
    {
        SentrySdk.CaptureException(exception);
    }

    private static CrashReportingConfig? Config()
    {
        // The baeium edition ships without crash reporting by design, so its
        // absence is expected and silent.
        if (!NativeBae.SupportsOAuthProviders())
        {
            return null;
        }
        return Config(
            AppAssembly.GetName().Version?.ToString(),
            AppMetadata.ConfiguredString("BaeSentryDsn"),
            AppMetadata.ConfiguredString("BaeEnvironment"),
            AppMetadata.ConfiguredString("BaeGitCommit"),
            BaeDiagnostics.Logger);
    }

    // The crash-reporting config the build's values make, or null — with a
    // warning naming what was missing — when any of them is missing.
    internal static CrashReportingConfig? Config(
        string? appVersion,
        string? dsn,
        string? environment,
        string? gitCommit,
        BaeLogger logger)
    {
        if (appVersion is null || dsn is null || environment is null || gitCommit is null)
        {
            var missing = new (string Name, string? Value)[]
            {
                ("the assembly version", appVersion),
                ("BaeSentryDsn", dsn),
                ("BaeEnvironment", environment),
                ("BaeGitCommit", gitCommit),
            }.Where(value => value.Value is null).Select(value => value.Name);
            logger.Warning(
                $"Crash reporting disabled: not configured at build time (missing {string.Join(", ", missing)}). " +
                "Crashes will not be reported.");
            return null;
        }
        return new CrashReportingConfig(
            dsn,
            environment,
            $"bae@{appVersion}+{gitCommit}",
            "bae",
            gitCommit);
    }

    private static void ConfigureNative(CrashReportingConfig config)
    {
        try
        {
            var appDir = AppContext.BaseDirectory;
            var handler = Path.Combine(
                appDir,
                OperatingSystem.IsWindows() ? "crashpad_handler.exe" : "crashpad_handler");
            if (!File.Exists(handler))
            {
                BaeDiagnostics.Logger.Error($"Sentry native handler missing at {handler}");
                return;
            }

            var options = NativeSentry.OptionsNew();
            NativeSentry.OptionsSetDsn(options, config.Dsn);
            NativeSentry.OptionsSetEnvironment(options, config.Environment);
            NativeSentry.OptionsSetRelease(options, config.Release);
            NativeSentry.OptionsSetDist(options, config.GitCommit);
            NativeSentry.OptionsSetDebug(options, 0);
            NativeSentry.OptionsSetAutoSessionTracking(options, 1);
            NativeSentry.OptionsSetHandlerPath(options, handler);
            var databasePath = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "bae",
                "sentry-native");
            Directory.CreateDirectory(databasePath);
            NativeSentry.OptionsSetDatabasePath(options, databasePath);

            if (NativeSentry.Init(options) != 0)
            {
                BaeDiagnostics.Logger.Error("Failed to configure native crash reporting");
                return;
            }
            NativeSentry.SetTag("edition", config.Edition);
            NativeSentry.SetTag("git_commit", config.GitCommit);
        }
        catch (Exception exception) when (
            exception is DllNotFoundException
                or EntryPointNotFoundException
                or BadImageFormatException)
        {
            BaeDiagnostics.Logger.Error("Failed to load native crash reporting", exception);
        }
    }
}

internal sealed record CrashReportingConfig(
    string Dsn,
    string Environment,
    string Release,
    string Edition,
    string GitCommit);

// The sentry-native C API. The library name is the bare "sentry-native": .NET's
// own probing turns it into sentry-native.dll on Windows and libsentry-native.so
// on Linux, both shipped beside the executable by the release lane.
internal static class NativeSentry
{
    [DllImport("sentry-native", EntryPoint = "sentry_options_new", CallingConvention = CallingConvention.Cdecl)]
    internal static extern nint OptionsNew();

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_dsn", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void OptionsSetDsn(nint options, string dsn);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_environment", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void OptionsSetEnvironment(nint options, string environment);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_release", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void OptionsSetRelease(nint options, string release);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_dist", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void OptionsSetDist(nint options, string dist);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_debug", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void OptionsSetDebug(nint options, int debug);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_auto_session_tracking", CallingConvention = CallingConvention.Cdecl)]
    internal static extern void OptionsSetAutoSessionTracking(nint options, int enabled);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_handler_path", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void OptionsSetHandlerPath(nint options, string path);

    [DllImport("sentry-native", EntryPoint = "sentry_options_set_database_path", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void OptionsSetDatabasePath(nint options, string path);

    [DllImport("sentry-native", EntryPoint = "sentry_init", CallingConvention = CallingConvention.Cdecl)]
    internal static extern int Init(nint options);

    [DllImport("sentry-native", EntryPoint = "sentry_set_tag", CallingConvention = CallingConvention.Cdecl, CharSet = CharSet.Ansi)]
    internal static extern void SetTag(string key, string value);
}

// The crash reporter core reports its bugs to — code that broke its own
// guarantee: each one a non-fatal Sentry event carrying the operation and the
// error's full text. Core has already logged it. Without the SDK initialized
// (the baeium edition, a test) the event goes nowhere. Core calls it from
// whatever thread found the bug; SentrySdk is thread-safe.
internal sealed class CoreBugReporter : BridgeBugReporter
{
    public void Report(string operation, string detail)
    {
        SentrySdk.CaptureException(
            new CoreBugException(operation, detail),
            scope => scope.SetExtra("operation", operation));
    }
}

// A bug core reported, as the exception Sentry records.
internal sealed class CoreBugException(string operation, string detail)
    : Exception($"{operation}: {detail}");

