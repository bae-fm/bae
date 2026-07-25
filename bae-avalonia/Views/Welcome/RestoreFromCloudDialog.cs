using System;
using System.Threading.Tasks;
using Avalonia.Controls;
using uniffi.bae_bridge;

namespace Bae.Desktop;

// Restore a library by entering its cloud location and credentials directly, for
// the S3 credential provider whose secrets a restore code can't carry. OAuth-
// backed libraries restore from a code instead, where the browser sign-in
// supplies the tokens (that path lands with the OAuth pickers in the parity
// port). Presented in the window's modal host.
internal sealed class RestoreFromCloudDialog
{
    private readonly Action _dismissWelcome;
    private readonly Action<string> _openLibrary;

    public RestoreFromCloudDialog(Action dismissWelcome, Action<string> openLibrary)
    {
        _dismissWelcome = dismissWelcome;
        _openLibrary = openLibrary;
    }

    public Control Build(Action close)
    {
        var column = DialogUi.Column();
        column.Children.Add(DialogUi.Title(Loc.Chrome("restore.from_cloud_title")));
        column.Children.Add(DialogUi.Field(Loc.Chrome("restore.field.library_id"), out var libraryIdBox));
        column.Children.Add(DialogUi.Field(Loc.Chrome("restore.field.encryption_key"), out var keyBox, secret: true));
        column.Children.Add(DialogUi.Field(Loc.Chrome("restore.field.library_name"), out var nameBox));
        column.Children.Add(DialogUi.Field(Loc.Chrome("s3.field.bucket"), out var bucketBox));
        column.Children.Add(DialogUi.Field(Loc.Chrome("s3.field.region"), out var regionBox));
        column.Children.Add(DialogUi.Field(Loc.Chrome("s3.field.endpoint"), out var endpointBox));
        column.Children.Add(DialogUi.Field(Loc.Chrome("s3.field.access_key"), out var accessKeyBox, secret: true));
        column.Children.Add(DialogUi.Field(Loc.Chrome("s3.field.secret_key"), out var secretKeyBox, secret: true));

        var status = DialogUi.Danger();
        var restore = DialogUi.Primary(Loc.Chrome("restore.confirm"));
        restore.IsEnabled = false;
        var cancel = new Button { Content = Loc.Chrome("action.cancel") };
        column.Children.Add(status);
        column.Children.Add(DialogUi.Actions(cancel, restore));

        void Revalidate() =>
            restore.IsEnabled =
                !string.IsNullOrWhiteSpace(libraryIdBox.Text)
                && !string.IsNullOrWhiteSpace(keyBox.Text)
                && !string.IsNullOrWhiteSpace(bucketBox.Text)
                && !string.IsNullOrWhiteSpace(regionBox.Text)
                && !string.IsNullOrWhiteSpace(accessKeyBox.Text)
                && !string.IsNullOrWhiteSpace(secretKeyBox.Text);

        foreach (var box in new[] { libraryIdBox, keyBox, bucketBox, regionBox, accessKeyBox, secretKeyBox })
        {
            box.TextChanged += (_, _) => Revalidate();
        }

        cancel.Click += (_, _) => close();
        restore.Click += async (_, _) =>
        {
            restore.IsEnabled = false;
            status.Text = Loc.Chrome("restore.in_progress");
            status.IsVisible = true;

            string restoredLibraryId;
            try
            {
                restoredLibraryId = await Task.Run(() => NativeBae.RestoreFromS3(
                    libraryIdBox.Text?.Trim() ?? string.Empty,
                    keyBox.Text?.Trim() ?? string.Empty,
                    nameBox.Text?.Trim() ?? string.Empty,
                    bucketBox.Text?.Trim() ?? string.Empty,
                    regionBox.Text?.Trim() ?? string.Empty,
                    string.IsNullOrWhiteSpace(endpointBox.Text) ? null : endpointBox.Text!.Trim(),
                    accessKeyBox.Text?.Trim() ?? string.Empty,
                    secretKeyBox.Text ?? string.Empty));
            }
            catch (BridgeException exception)
            {
                BaeDiagnostics.Logger.Error("Failed to restore S3 library.", exception);
                status.Text = Loc.Chrome("restore.failed");
                restore.IsEnabled = true;
                return;
            }

            close();
            _dismissWelcome();
            _openLibrary(restoredLibraryId);
        };

        return new ScrollViewer { Content = column, MaxHeight = 560 };
    }
}
