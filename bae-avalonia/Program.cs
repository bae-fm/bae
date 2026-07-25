using Avalonia;
using Velopack;
#if DEBUG
using Avalonia.Headless;
#endif

namespace Bae.Desktop;

// Plain Avalonia entry point: the process-wide crash hooks, the Velopack hook,
// the single-instance election that forwards a second launch's argv to the
// running instance, then the desktop lifetime. In DEBUG the --capture-shots flag
// diverts to the off-screen headless render path before any window opens.
internal static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
        // Record any unhandled exception from process entry onward — before the
        // Velopack hook, the single-instance election, or the app exists — so a
        // startup crash leaves a managed record behind.
        CrashCapture.InstallProcessHandlers();

#if DEBUG
        // Capture mode serves a one-shot render, not a real session: no
        // single-instance redirect (a running instance must not divert it), no
        // window.
        if (ShotCapture.TryGetOutputDir(args, out var outputDir))
        {
            BuildHeadlessAvaloniaApp().SetupWithoutStarting();
            return ShotCapture.Run(args, outputDir);
        }

        // Live-composition smoke: create a real library and render the shell over
        // it (touches the credential store and ~/.bae — throwaway machines only).
        if (SmokeCreate.TryGetOutputDir(args, out var smokeDir))
        {
            BuildHeadlessAvaloniaApp().SetupWithoutStarting();
            return SmokeCreate.Run(smokeDir);
        }
#endif

        // Handles the Velopack install / update / uninstall hook arguments and
        // exits the process for them, so it runs before anything else initializes
        // — and before the single-instance election, which a hook run must not
        // take part in.
        var velopack = VelopackApp.Build();
        if (OperatingSystem.IsWindows())
        {
            // The pre-uninstall hook takes back the OS handler registration every
            // normal launch asserts; it runs ahead of any resource initialization,
            // so it must not reach for a localized string. Windows is the only
            // platform with an uninstaller to run it — Velopack never triggers this
            // hook elsewhere.
            velopack.OnBeforeUninstallFastCallback(_ => ProtocolRegistration.Unregister());
        }
        velopack.Run();

        // One instance per edition. A second launch forwards its argv (a bae://
        // URL or file/folder args) to the running instance and exits.
        var edition = App.Edition;
        var single = SingleInstance.Acquire(edition, args, App.OnRedirectedActivation);
        if (single is null)
        {
            return 0;
        }

        App.SingleInstance = single;
        // An exception that escapes the main loop is an unhandled exception, so
        // the process hook installed above already records it; the dispatcher
        // hook records UI-thread failures at the throw site, before the unwind
        // passes through the backend's native frames. Catching here as well would
        // only write the same crash a third time.
        return BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);
    }

    // Referenced by the Avalonia design previewer as well as Main.
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();

#if DEBUG
    // The off-screen render platform for shot capture: real Skia drawing on the
    // headless surface (no compositor, no display), so RenderTargetBitmap-style
    // capture works on any runner.
    private static AppBuilder BuildHeadlessAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UseSkia()
            .UseHeadless(new AvaloniaHeadlessPlatformOptions { UseHeadlessDrawing = false })
            .WithInterFont();
#endif
}
