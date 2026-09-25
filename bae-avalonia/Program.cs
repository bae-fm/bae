using Avalonia;
using Velopack;

namespace Bae.Desktop;

// Plain Avalonia entry point: the process-wide crash hooks, the Velopack hook,
// the single-instance election that forwards a second launch's argv to the
// running instance, then the desktop lifetime.
internal static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
        // Record any unhandled exception from process entry onward — before the
        // Velopack hook, the single-instance election, or the app exists — so a
        // startup crash leaves a managed record behind.
        CrashCapture.InstallProcessHandlers();

        // Handles the Velopack install / update / uninstall hook arguments and
        // exits the process for them, so it runs before anything else initializes
        // — and before the single-instance election, which a hook run must not
        // take part in.
        VelopackApp.Build().Run();

        // One instance per edition. A second launch forwards its argv to the
        // running instance and exits.
        var single = SingleInstance.Acquire(App.Edition, args, App.OnRedirectedActivation);
        if (single is null)
        {
            return 0;
        }

        App.SingleInstance = single;
        // An exception that escapes the main loop is an unhandled exception, so
        // the process hook installed above already records it; the dispatcher
        // hook records UI-thread failures at the throw site, before the unwind
        // passes through the backend's native frames.
        return BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    // Referenced by the Avalonia design previewer as well as Main.
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
