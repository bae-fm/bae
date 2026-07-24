using System;
using System.IO;
using System.Threading.Tasks;
using Microsoft.UI.Xaml;

namespace Bae.Windows;

// Records otherwise-anonymous managed crashes. Without these hooks a managed
// exception on the UI dispatcher, a background thread, or an unobserved task dies
// as a stowed native crash (0xc000027b) with no managed detail in the dump. Each
// hook appends the exception to a crash log under the app's local data dir and
// forwards it to the existing Sentry path. The XAML hook does not mark the
// exception handled — the process still crashes as it would have; we only want
// the record. A real app feature (not DEBUG-gated); harmless in capture mode.
internal static class CrashCapture
{
    // Beside the app's other local state — the sentry-native database lives under
    // this same %LOCALAPPDATA%\bae directory.
    private static readonly string CrashLogPath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "bae",
        "crash.log");

    // The two process-global sources: any thread's unhandled exception, and any
    // faulted task whose exception was never observed. Wired once at process
    // entry so they cover startup before the Application exists.
    internal static void InstallProcessHandlers()
    {
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
            Record("AppDomain.UnhandledException", e.ExceptionObject as Exception);
        TaskScheduler.UnobservedTaskException += (_, e) =>
            Record("TaskScheduler.UnobservedTaskException", e.Exception);
    }

    // The XAML source: an unhandled exception in a UI-dispatcher callback. It
    // needs the Application instance, so it is wired from the App constructor.
    // e.Handled is left false on purpose — record, then let it crash.
    internal static void InstallXamlHandler(Application app)
    {
        app.UnhandledException += (_, e) =>
            Record("Application.UnhandledException", e.Exception);
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
