using Avalonia;
using Velopack;
#if DEBUG
using Avalonia.Headless;
#endif

namespace Bae.Desktop;

// Plain Avalonia entry point: the Velopack hook, the single-instance election
// that forwards a second launch's argv to the running instance, then the desktop
// lifetime. In DEBUG the --capture-shots flag diverts to the off-screen headless
// render path before any window opens.
internal static class Program
{
    [STAThread]
    public static int Main(string[] args)
    {
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
        VelopackApp.Build().Run();

        // One instance per edition. A second launch forwards its argv (a bae://
        // URL or file/folder args) to the running instance and exits.
        var edition = App.Edition;
        var single = SingleInstance.Acquire(edition, args, App.OnRedirectedActivation);
        if (single is null)
        {
            return 0;
        }

        App.SingleInstance = single;
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
