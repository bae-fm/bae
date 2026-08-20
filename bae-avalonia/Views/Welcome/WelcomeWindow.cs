using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Layout;
using Avalonia.Markup.Xaml.MarkupExtensions;
using Avalonia.Media;
using uniffi.bae_bridge;

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
    private readonly TextBlock _statusDetail;
    private readonly StackPanel _statusColumn;
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

        _status = new TextBlock { TextAlignment = TextAlignment.Center };
        _status[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        // The fault under the line, when the failure carried one. Core's line
        // names a category ("Something went wrong."), so on its own it tells a
        // user with an unopenable library nothing about why — the same reason
        // the sync failure row is two lines.
        _statusDetail = new TextBlock
        {
            TextAlignment = TextAlignment.Center,
            TextWrapping = TextWrapping.Wrap,
            FontFamily = new FontFamily("monospace"),
            FontSize = 12,
            IsVisible = false,
        };
        _statusDetail[!TextBlock.ForegroundProperty] = new DynamicResourceExtension("BaeTextSecondaryBrush");

        _statusColumn = new StackPanel
        {
            Spacing = 4,
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Top,
            Margin = new Avalonia.Thickness(24, 28, 24, 0),
        };
        _statusColumn.Children.Add(_status);
        _statusColumn.Children.Add(_statusDetail);

        void SetStatus(string text) => SetStatusLines(text, null);

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
        root.Children.Add(_statusColumn);
        root.Children.Add(_modalHost);
        Content = root;

        Opened += async (_, _) =>
        {
            try
            {
                var pending = BaeBridgeMethods.PendingDevicePairingJoin();
                if (pending is not null)
                {
                    await _modalHost.Show(close => _joinDialog.Build(close, pending));
                }
            }
            catch (uniffi.bae_bridge.BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to resume pending device pairing.", exception);
                SetStatus(Loc.Chrome("join.failed"));
            }
        };
    }

    // Overwrite the status line — used to surface a failed library open when the
    // window is already up. `detail` is the untranslated diagnostic under it,
    // absent when the failure carried none.
    public void SetStatus(string text, string? detail = null) => SetStatusLines(text, detail);

    private void SetStatusLines(string text, string? detail)
    {
        _status.Text = text;
        _statusDetail.Text = detail ?? string.Empty;
        _statusDetail.IsVisible = detail is not null;
    }

    // Prompt for the encryption key of a locked library. On success the unlock
    // dialog re-opens the library through the coordinator.
    public Task ShowUnlock() =>
        _modalHost.Show(_unlockDialog.Build);
}
