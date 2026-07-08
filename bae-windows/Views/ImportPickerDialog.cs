using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using uniffi.bae_bridge;

namespace Bae.Windows;

// Import-confirm dialog for a candidate: pick an identity (the auto-identified
// matches, or one found by manual search when auto-identification came up empty),
// preview the candidate's audio, choose a storage mode, and import.
internal sealed class ImportPickerDialog
{
    private readonly SessionStore _session;
    private readonly Func<XamlRoot?> _xamlRoot;
    private readonly ImportStore _import;
    private readonly ImportConfirmDialog _confirm;

    public ImportPickerDialog(
        SessionStore session,
        Func<XamlRoot?> xamlRoot,
        ImportStore import,
        ImportConfirmDialog confirm)
    {
        _session = session;
        _xamlRoot = xamlRoot;
        _import = import;
        _confirm = confirm;
    }

    public async System.Threading.Tasks.Task Show(ImportCandidate candidate)
    {
        var results = new List<ReleaseCandidateChoice>(candidate.Matches);
        var resultsList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 240,
        };
        void RenderResults() => resultsList.ItemsSource = results.Select(result => result.Summary).ToList();
        RenderResults();

        var artistBox = new TextBox { Header = Loc.Chrome("import.field.artist_manual") };
        var albumBox = new TextBox { Header = Loc.Chrome("search.field.album"), Text = candidate.Name };
        var sourceBox = new ComboBox { Header = Loc.Chrome("search.field.source") };
        sourceBox.Items.Add("discogs");
        sourceBox.Items.Add("musicbrainz");
        sourceBox.SelectedIndex = 0;
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(new TextBlock { Text = candidate.Name });

        // Preview the candidate's audio before committing to an identity. The live
        // position label follows the import store's preview state while open.
        Action? renderPreview = null;
        if (candidate.AudioPaths.Count > 0)
        {
            var preview = new Button { Content = "▶ " + Loc.Chrome("import.preview") };
            preview.Click += (_, _) => _session.WithCurrentHandle(
                handle => NativeBae.PreviewPlay(handle, candidate.AudioPaths[0]));
            var pause = new Button { Content = "⏸" };
            pause.Click += (_, _) => _session.WithCurrentHandle(NativeBae.PreviewTogglePause);
            var stop = new Button { Content = "⏹" };
            stop.Click += (_, _) => _session.WithCurrentHandle(NativeBae.PreviewStop);
            // Live preview position, updated by PreviewProgress while the picker is open.
            var previewElapsed = new TextBlock { VerticalAlignment = VerticalAlignment.Center };
            renderPreview = () => previewElapsed.Text = _import.PreviewElapsedText;
            _import.PreviewElapsedChanged += renderPreview;
            var previewRow = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            previewRow.Children.Add(preview);
            previewRow.Children.Add(pause);
            previewRow.Children.Add(stop);
            previewRow.Children.Add(previewElapsed);
            content.Children.Add(previewRow);
        }

        content.Children.Add(resultsList);
        content.Children.Add(artistBox);
        content.Children.Add(albumBox);
        content.Children.Add(sourceBox);
        content.Children.Add(searchButton);
        content.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("import.release_title"),
            Content = new ScrollViewer { Content = content },
            PrimaryButtonText = Loc.Chrome("action.import"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
            IsPrimaryButtonEnabled = false,
        };

        searchButton.Click += async (_, _) =>
        {
            var source = (string)sourceBox.SelectedItem;
            var artist = artistBox.Text;
            var album = albumBox.Text;
            searchButton.IsEnabled = false;
            var (current, search) = await _session.RunForCurrentHandle(
                handle => NativeBae.SearchReleases(handle, source, artist, album));
            searchButton.IsEnabled = true;
            if (!current)
            {
                return;
            }
            if (search.Error is not null)
            {
                status.Text = search.Error;
                status.Visibility = Visibility.Visible;
                return;
            }

            results = search.Candidates ?? [];
            RenderResults();
            dialog.IsPrimaryButtonEnabled = false;
        };

        resultsList.SelectionChanged += (_, _) =>
        {
            dialog.IsPrimaryButtonEnabled = resultsList.SelectedIndex >= 0;
        };

        // Clicking Import here doesn't commit — it advances to the metadata edit
        // step. A second ContentDialog can't open while this one shows, so the
        // picker closes on Primary (the selection + storage mode are captured) and
        // the confirm step opens after ShowAsync returns. The loop re-opens this
        // picker — with its search results and selection intact — when the confirm
        // step's "Back to Search" is chosen; Import or Cancel ends the flow.
        try
        {
            while (true)
            {
                var pickerResult = await dialog.ShowAsync();
                _session.WithCurrentHandle(NativeBae.PreviewStop);
                if (pickerResult != ContentDialogResult.Primary)
                {
                    return;
                }

                var index = resultsList.SelectedIndex;
                if (index < 0 || index >= results.Count)
                {
                    return;
                }

                var backToSearch = await _confirm.Show(candidate, results[index]);
                if (!backToSearch)
                {
                    return;
                }
            }
        }
        finally
        {
            if (renderPreview is not null)
            {
                _import.PreviewElapsedChanged -= renderPreview;
            }
            _import.ClearPreview();
        }
    }
}
