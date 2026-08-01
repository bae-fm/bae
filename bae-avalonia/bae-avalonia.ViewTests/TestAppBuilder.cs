using Avalonia;
using Avalonia.Headless;

[assembly: AvaloniaTestApplication(typeof(Bae.Desktop.ViewTests.TestAppBuilder))]

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The headless application every view test runs against. Avalonia's xunit
/// integration reads this off the assembly attribute above, starts one session
/// around it, and runs each <c>[AvaloniaFact]</c> body on that session's
/// dispatcher thread.
///
/// That thread is the whole point. A control's constructor calls
/// <c>Dispatcher.VerifyAccess</c>, and xunit hands each test whatever worker
/// thread is free, so a suite that merely installs the platform once and then
/// builds controls wherever it lands passes or fails on thread luck.
/// </summary>
internal static class TestAppBuilder
{
    public static AppBuilder BuildAvaloniaApp() =>
        AppBuilder
            .Configure<App>()
            .UseHeadless(new AvaloniaHeadlessPlatformOptions());
}
