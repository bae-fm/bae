using Avalonia.Controls;
using Avalonia.Media;
using Avalonia.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal static class EvidenceDialog
{
    internal static async Task Show(ModalHost host, EvidenceService service,
        BridgeEvidenceSubject subject, BridgeEvidenceSelection selection,
        Action<string, string> showError)
    {
        var (current, result) = await service.Read(subject, selection);
        if (!current) return;
        if (result.Error is { } error)
        {
            showError(Loc.Chrome("error.title"), error);
            return;
        }
        var contents = result.Contents ?? throw new InvalidOperationException("Evidence read returned no contents");
        if (contents is [BridgeEvidenceContent.Reveal revealOnly])
        {
            RevealInFileManager.Reveal(revealOnly.Path);
            return;
        }
        var bitmaps = new List<Bitmap>();
        try
        {
            await host.Show(close =>
            {
                var column = DialogUi.Column();
                column.Width = 720;
                var tabs = new List<TabItem>();
                foreach (var content in contents)
                {
                    switch (content)
                    {
                        case BridgeEvidenceContent.Document document:
                            tabs.Add(new TabItem
                            {
                                Header = document.Name,
                                Content = new ScrollViewer
                                {
                                    Height = 480,
                                    Content = new SelectableTextBlock
                                    {
                                        Text = document.Text,
                                        FontFamily = new FontFamily("monospace"),
                                        TextWrapping = TextWrapping.Wrap
                                    },
                                }
                            });
                            break;
                        case BridgeEvidenceContent.Image image:
                            {
                                using var stream = new System.IO.MemoryStream(image.Bytes);
                                var bitmap = new Bitmap(stream);
                                bitmaps.Add(bitmap);
                                tabs.Add(new TabItem
                                {
                                    Header = image.Name,
                                    Content = new Image { Source = bitmap, Height = 480, Stretch = Stretch.Uniform }
                                });
                                break;
                            }
                        case BridgeEvidenceContent.Reveal reveal:
                            var button = new Button { Content = reveal.Path };
                            button.Click += (_, _) => RevealInFileManager.Reveal(reveal.Path);
                            tabs.Add(new TabItem { Header = reveal.Path, Content = button });
                            break;
                        default:
                            throw new ArgumentOutOfRangeException(nameof(content));
                    }
                }
                column.Children.Add(new TabControl { ItemsSource = tabs, SelectedIndex = 0 });
                var done = DialogUi.Primary(Loc.Chrome("action.close"));
                done.Click += (_, _) => close();
                column.Children.Add(DialogUi.Actions(done));
                return column;
            });
        }
        catch (Exception exception)
        {
            BaeDiagnostics.Logger.Warning("Could not display release evidence.", exception);
            showError(Loc.Chrome("error.title"), exception.Message);
        }
        finally
        {
            foreach (var bitmap in bitmaps) bitmap.Dispose();
        }
    }
}
