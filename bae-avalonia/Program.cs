using Avalonia;
#if DEBUG
using Avalonia.Headless;
#endif

namespace Bae.Desktop;

// Plain Avalonia entry point. Single-instance forwarding and activation intents
// arrive with the welcome/main windows; this stays the bare bootstrap. In DEBUG
// the --capture-shots flag diverts to the off-screen headless render path before
// any window opens.
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
#endif

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
