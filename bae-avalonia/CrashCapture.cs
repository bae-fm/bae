using System;
using System.IO;
using System.Threading.Tasks;
using Avalonia.Threading;

namespace Bae.Desktop;

// Records otherwise-anonymous managed crashes. Without these hooks an exception
// on the UI thread, on a background thread, or in an unobserved task takes the
// process down leaving nothing but an exit code behind. Each hook appends the
// exception to a crash log under the app's local data directory and forwards it
// to the Sentry path. No hook marks the exception handled or observed — what
// happens next happens exactly as it would have; we only want the record.
internal static class CrashCapture
{
    // Beside the app's other local state — the sentry-native database lives under
    // this same bae directory.
    private static readonly string CrashLogPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "crash.log");

    // The two process-global sources: any thread's unhandled exception, and any
    // faulted task whose exception was never observed. Wired once at process
    // entry so they cover startup before the app exists.
    internal static void InstallProcessHandlers()
    {
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
            Record("AppDomain.UnhandledException", e.ExceptionObject as Exception);
        TaskScheduler.UnobservedTaskException += (_, e) =>
            Record("TaskScheduler.UnobservedTaskException", e.Exception);
    }

    // The UI-thread source: an exception escaping a job the dispatcher ran.
    // Avalonia rethrows it into the main loop as long as no handler marks it
    // handled, so e.Handled is left false on purpose; the record is taken here
    // rather than left to the main loop because the rethrow unwinds through the
    // windowing backend's native message-loop frames first, which a managed
    // exception does not reliably survive. Wired from the running app, not from
    // process entry: a Dispatcher.UIThread touched before the windowing platform
    // is initialized binds itself to a do-nothing implementation for the rest of
    // the process.
    internal static void InstallDispatcherHandler()
    {
        Dispatcher.UIThread.UnhandledException += (_, e) =>
            Record("Dispatcher.UnhandledException", e.Exception);
    }

    private static void Record(string source, Exception? exception)
    {
        try
        {
            var detail = exception?.ToString() ?? "(no exception object)";
            var entry =
                $"{DateTime.UtcNow:o} [{source}]{Environment.NewLine}{detail}{Environment.NewLine}{Environment.NewLine}";
            Directory.CreateDirectory(Path.GetDirectoryName(CrashLogPath)!);
            File.AppendAllText(CrashLogPath, entry);
        }
        catch (Exception)
        {
            // A crash handler must never throw.
        }

        try
        {
            if (exception is not null)
            {
                BaeCrashReporting.CaptureException(exception);
            }
        }
        catch (Exception)
        {
            // Sentry forwarding is best-effort and must not throw from the handler.
        }
    }
}
