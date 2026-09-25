using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Threading;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Brings up the process-lifetime services — telemetry, crash reporting, the
// bridge host, the background update check — and opens the skeleton window over
// the host. Every quit flushes telemetry before the process exits.
public sealed partial class App : Application
{
    private BridgeHost? _host;
    private MainWindow? _main;

    // Set once the exit flush has started. The shutdown that follows it is
    // forced and so never re-enters the handler, but a second quit request
    // arriving while the flush is still running would, and must not run it
    // twice.
    private bool _quitting;

    // The edition key for single-instancing and per-edition state. The full
    // (OAuth) edition compiles with BAE_FULL_BRIDGE; the baeium edition doesn't.
    internal static string Edition =>
#if BAE_FULL_BRIDGE
        "bae";
#else
        "baeium";
#endif

    // Held for the process lifetime so its listener keeps accepting forwarded
    // launches; disposed implicitly on exit.
    internal static SingleInstance? SingleInstance { get; set; }

    // A second launch's argv, on the single-instance listener's background
    // thread: marshal to the UI thread and bring the window forward.
    internal static void OnRedirectedActivation(IReadOnlyList<string> args) =>
        Dispatcher.UIThread.Post(() => (Current as App)?.HandleRedirectedActivation(args));

    private void HandleRedirectedActivation(IReadOnlyList<string> args)
    {
        BaeDiagnostics.Logger.Info($"A second launch forwarded {args.Count} argument(s).");
        _main?.Activate();
    }

    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        // The windowing platform is up by now, so the UI dispatcher is the real
        // one; the hook has to be in place before the first job runs through it.
        CrashCapture.InstallDispatcherHandler();

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            // Every quit funnels through here: the last window closing and the
            // OS asking the app to exit.
            desktop.ShutdownRequested += (_, args) => QuitAfterFlush(desktop, args);
            _host = StartSession();
            _main = new MainWindow(_host);
            desktop.MainWindow = _main;
        }

        base.OnFrameworkInitializationCompleted();
    }

    // Telemetry first (so the sink exists for every later step and any failure it
    // reports), then crash reporting, then the host every bridge call runs over.
    // The update check is silent and fire-and-forget: it logs every failure and
    // never blocks startup.
    private static BridgeHost StartSession()
    {
        var appDir = NativeBae.UserAppDir();
        BaeDiagnostics.Configure();
        BaeCrashReporting.Configure();
        BaeDiagnostics.Logger.Info("application launched");
        var host = NativeBae.CreateHost(BaeDiagnostics.Handle, appDir);
        _ = UpdateService.StageInBackgroundAsync();
        return host;
    }

    // Flush buffered telemetry, then quit. The flush is asynchronous and the
    // process must not exit at the first await, so the shutdown that asked is
    // cancelled, the flush runs to completion, and only then does the app shut
    // down for real — that second shutdown is forced, so nothing can cancel it
    // and it does not come back through here.
    private async void QuitAfterFlush(
        IClassicDesktopStyleApplicationLifetime desktop, ShutdownRequestedEventArgs args)
    {
        args.Cancel = true;
        if (_quitting)
        {
            return;
        }

        _quitting = true;
        if (_host is { } host)
        {
            await BaeDiagnostics.Flush(host);
        }
        desktop.Shutdown();
    }
}
