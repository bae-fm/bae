using System;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Restore a library by entering its cloud location and credentials directly, for
// the S3 credential provider whose secrets a restore code can't carry.
// OAuth-backed libraries restore from a code instead, where the browser sign-in
// supplies the tokens.
internal sealed class RestoreFromCloudDialog
{
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    public RestoreFromCloudDialog(Func<XamlRoot?> xamlRoot, Action dismissWelcome, Action<string> openLibrary)
    {
        _xamlRoot = xamlRoot;
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public async System.Threading.Tasks.Task Show()
    {
        var content = new StackPanel { Spacing = 8, MinWidth = 360 };

        var libraryIdBox = new TextBox { Header = Loc.Chrome("restore.field.library_id") };
        // The encryption key unlocks the whole library — mask it, as macOS does.
        var keyBox = new PasswordBox { Header = Loc.Chrome("restore.field.encryption_key") };
        var nameBox = new TextBox { Header = Loc.Chrome("restore.field.library_name") };
        content.Children.Add(libraryIdBox);
        content.Children.Add(keyBox);
        content.Children.Add(nameBox);

        var providerPicker = new ComboBox
        {
            Header = Loc.Chrome("restore.field.cloud_storage"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
        };
        foreach (var wire in new[] { "s3" })
        {
            providerPicker.Items.Add(new ComboBoxItem { Content = BridgeDisplay.ProviderDisplayName(wire), Tag = wire });
        }
        content.Children.Add(providerPicker);

        // S3 fields, shown when S3 is selected.
        var s3Bucket = new TextBox { Header = Loc.Chrome("s3.field.bucket"), Visibility = Visibility.Collapsed };
        var s3Region = new TextBox { Header = Loc.Chrome("s3.field.region"), Visibility = Visibility.Collapsed };
        var s3Endpoint = new TextBox { Header = Loc.Chrome("s3.field.endpoint"), Visibility = Visibility.Collapsed };
        var s3AccessKey = new PasswordBox { Header = Loc.Chrome("s3.field.access_key"), Visibility = Visibility.Collapsed };
        var s3SecretKey = new PasswordBox { Header = Loc.Chrome("s3.field.secret_key"), Visibility = Visibility.Collapsed };
        content.Children.Add(s3Bucket);
        content.Children.Add(s3Region);
        content.Children.Add(s3Endpoint);
        content.Children.Add(s3AccessKey);
        content.Children.Add(s3SecretKey);

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        content.Children.Add(status);

        var restoreButton = new Button
        {
            Content = Loc.Chrome("restore.confirm"),
            HorizontalAlignment = HorizontalAlignment.Stretch,
            IsEnabled = false,
        };
        content.Children.Add(restoreButton);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("restore.from_cloud_title"),
            Content = new ScrollViewer { Content = content, MaxHeight = 520 },
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
        };

        string SelectedWire() =>
            (providerPicker.SelectedItem as ComboBoxItem)?.Tag as string ?? string.Empty;

        // Enable restore only when the common fields and the selected provider's
        // required fields are all filled.
        void Revalidate()
        {
            var wire = SelectedWire();
            var common = !string.IsNullOrWhiteSpace(libraryIdBox.Text)
                && !string.IsNullOrWhiteSpace(keyBox.Password);
            var providerReady = wire switch
            {
                "s3" => !string.IsNullOrWhiteSpace(s3Bucket.Text)
                    && !string.IsNullOrWhiteSpace(s3Region.Text)
                    && !string.IsNullOrWhiteSpace(s3AccessKey.Password)
                    && !string.IsNullOrWhiteSpace(s3SecretKey.Password),
                _ => false,
            };
            restoreButton.IsEnabled = common && providerReady;
        }

        providerPicker.SelectionChanged += (_, _) =>
        {
            var s3 = SelectedWire() == "s3";
            s3Bucket.Visibility = s3Region.Visibility = s3Endpoint.Visibility =
                s3AccessKey.Visibility = s3SecretKey.Visibility =
                    s3 ? Visibility.Visible : Visibility.Collapsed;
            Revalidate();
        };
        foreach (var box in new[] { libraryIdBox, s3Bucket, s3Region })
        {
            box.TextChanged += (_, _) => Revalidate();
        }
        foreach (var secret in new[] { keyBox, s3AccessKey, s3SecretKey })
        {
            secret.PasswordChanged += (_, _) => Revalidate();
        }
        // Land on S3 so its fields show immediately; fires the handler above.
        providerPicker.SelectedIndex = 0;

        restoreButton.Click += async (_, _) =>
        {
            restoreButton.IsEnabled = false;
            status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            status.Text = Loc.Chrome("restore.in_progress");
            status.Visibility = Visibility.Visible;

            var libraryId = libraryIdBox.Text?.Trim() ?? string.Empty;
            var key = keyBox.Password?.Trim() ?? string.Empty;
            var name = nameBox.Text?.Trim() ?? string.Empty;
            string restoredLibraryId;
            try
            {
                restoredLibraryId = await System.Threading.Tasks.Task.Run(
                    () => NativeBae.RestoreFromS3(
                        libraryId,
                        key,
                        name,
                        s3Bucket.Text?.Trim() ?? string.Empty,
                        s3Region.Text?.Trim() ?? string.Empty,
                        s3Endpoint.Text?.Trim(),
                        s3AccessKey.Password?.Trim() ?? string.Empty,
                        s3SecretKey.Password ?? string.Empty));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to restore S3 library.", exception);
                status.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon);
                status.Text = Loc.Chrome("restore.failed");
                restoreButton.IsEnabled = true;
                return;
            }

            dialog.Hide();
            _dismissWelcome();
            _openLibrary(restoredLibraryId);
        };

        await dialog.ShowAsync();
    }
}
