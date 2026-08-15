using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;

namespace Bae.Desktop;

// The pre-library welcome window: a fixed-size window shown at launch when no
// library opens, and again after a library closes. It hosts the welcome chooser
// (WelcomeView) plus the join / restore / unlock dialogs, presented in its modal
// host. Opening a library swaps to a freshly constructed main window (the
// coordinator closes this window); this window never persists its bounds.
internal sealed class WelcomeWindow : Window
{
    private const int WidthDips = 900;
    private const int HeightDips = 600;

    private readonly TextBlock _status;
    private readonly ModalHost _modalHost = new();
    private readonly JoinLibraryDialog _joinDialog;
    private readonly RestoreFromCloudDialog _restoreDialog;
    private readonly UnlockDialog _unlockDialog;

    // openLibrary is the coordinator's open (App.OpenLibrary): every path here —
    // the chooser, create, restore, join, and unlock — hands it a library id and
    // lets the coordinator swap to the main window.
    public WelcomeWindow(
        Action<string> openLibrary,
        Func<string, Task<string?>> unlock,
        Action onUnlocked,
        Func<Task> cancelUnlock)
    {
        Title = "bae";
        Width = WidthDips;
        Height = HeightDips;
        CanResize = false;
        WindowStartupLocation = WindowStartupLocation.CenterScreen;

        _status = new TextBlock
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Avalonia.Thickness(0, 28, 0, 0),
            TextAlignment = TextAlignment.Center,
        };
        _status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        void SetStatus(string text) => _status.Text = text;

        _joinDialog = new JoinLibraryDialog(dismissWelcome: () => { }, openLibrary);
        _restoreDialog = new RestoreFromCloudDialog(dismissWelcome: () => { }, openLibrary);
        _unlockDialog = new UnlockDialog(SetStatus, unlock, onUnlocked, cancelUnlock);

        var welcomeView = new WelcomeView(
            SetStatus,
            () => LibraryDiscovery.Load(SetStatus),
            reportError => LibraryDiscovery.Create(reportError),
            openLibrary,
            () => _modalHost.Show(close => _joinDialog.Build(close)),
            () => _modalHost.Show(close => _restoreDialog.Build(close)));

        var root = new Panel();
        root[!BackgroundProperty] = new DynamicResourceExtension("BaeBackgroundBrush");
        root.Children.Add(welcomeView);
        root.Children.Add(_status);
        root.Children.Add(_modalHost);
        Content = root;
    }

    // Overwrite the status line — used to surface a failed library open when the
    // window is already up.
    public void SetStatus(string text) => _status.Text = text;

    // Prompt for the encryption key of a locked library. On success the unlock
    // dialog re-opens the library through the coordinator.
    public Task ShowUnlock() =>
        _modalHost.Show(_unlockDialog.Build);
}
