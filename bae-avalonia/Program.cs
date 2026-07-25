using Avalonia;

namespace Bae.Desktop;

// Plain Avalonia entry point. Single-instance forwarding and activation intents
// arrive with the welcome/main windows; this stays the bare bootstrap.
internal static class Program
{
    [STAThread]
    public static void Main(string[] args) =>
        BuildAvaloniaApp().StartWithClassicDesktopLifetime(args);

    // Referenced by the Avalonia design previewer as well as Main.
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .WithInterFont()
            .LogToTrace();
}
