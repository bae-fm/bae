using Microsoft.UI.Xaml;

namespace Bae.Windows;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        // Register the OS credential store before any library key is read or
        // written (discovery, creation, or open).
        NativeBae.Startup();

        // Register the bundled OAuth client credentials so coven can run the cloud
        // sign-in flows. Absent file: cloud sign-in stays unavailable.
        OAuthCreds.Register();

        _window = new MainWindow();
        _window.Activate();
    }
}
