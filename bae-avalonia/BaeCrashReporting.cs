using System;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using Sentry;

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
        if (!NativeBae.SupportsOAuthProviders())
        {
            return null;
        }
        var appVersion = AppAssembly.GetName().Version;
        var dsn = AppMetadata.ConfiguredString("BaeSentryDsn");
        var environment = AppMetadata.ConfiguredString("BaeEnvironment");
        var gitCommit = AppMetadata.ConfiguredString("BaeGitCommit");
        if (appVersion is null || dsn is null || environment is null || gitCommit is null)
        {
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
