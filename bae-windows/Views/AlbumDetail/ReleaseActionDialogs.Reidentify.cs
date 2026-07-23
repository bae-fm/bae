using System;
using System.Collections.Generic;
using System.Linq;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using uniffi.bae_bridge;

namespace Bae.Windows;

// The re-identify flow: the search-and-pick dialog and the follow-up prompt to
// reseed metadata after a source-backed commit. Split out of
// ReleaseActionDialogs.cs unchanged.
internal sealed partial class ReleaseActionDialogs
{
    public async System.Threading.Tasks.Task ShowReidentify(string releaseId, string seedArtist, string seedAlbum)
    {
        if (string.IsNullOrEmpty(releaseId))
        {
            return;
        }

        // The candidate key the identify pipeline runs under for this release.
        // Core treats it as an opaque key; the same string drives the toolbar
        // toggle/re-run wrappers and the snapshot read-back.
        var key = "reidentify:" + releaseId;

        // The live pipeline status line (gray) and the signals-toolbar host,
        // shown above the manual-search fallback. The pipeline starts with the
        // dialog, so the status seeds to "identifying…" until its first event.
        var pipelineStatus = new TextBlock
        {
            Text = Loc.Chrome("identify.identifying"),
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray),
            TextWrapping = TextWrapping.Wrap,
        };
        var badgeHost = new StackPanel();

        var artistBox = new TextBox { Header = Loc.Chrome("search.field.artist"), Text = seedArtist };
        var albumBox = new TextBox { Header = Loc.Chrome("search.field.album"), Text = seedAlbum };
        var sourceBox = new ComboBox { Header = Loc.Chrome("search.field.source") };
        sourceBox.Items.Add("discogs");
        sourceBox.Items.Add("musicbrainz");
        sourceBox.SelectedIndex = 0;
        var searchButton = new Button { Content = Loc.Chrome("action.search") };

        var resultsList = new ListView
        {
            SelectionMode = ListViewSelectionMode.Single,
            MaxHeight = 280,
        };
        var status = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };

        // The identity claim for the selected pressing: exact by default, or
        // metadata only. Hidden until a result is picked, and reset to exact on
        // every new pick. A skip-identify re-identify (the Secondary button) claims
        // nothing here — core reseeds the rows from the rip's file tags.
        var exactRadio = new RadioButton
        {
            Content = Loc.Chrome("identify.exact_pressing"),
            GroupName = "reidentifyIdentityClaim",
            IsChecked = true,
        };
        var metadataRadio = new RadioButton
        {
            Content = Loc.Chrome("identify.metadata_only"),
            GroupName = "reidentifyIdentityClaim",
        };
        var claimRow = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
            VerticalAlignment = VerticalAlignment.Center,
            Visibility = Visibility.Collapsed,
        };
        claimRow.Children.Add(new TextBlock
        {
            Text = Loc.Chrome("album.reidentify.claim_header"),
            VerticalAlignment = VerticalAlignment.Center,
        });
        claimRow.Children.Add(exactRadio);
        claimRow.Children.Add(metadataRadio);

        var identity = new ImportIdentityModel(hasSourceRelease: true);
        exactRadio.Checked += (_, _) => identity.SetMetadataOnly(false);
        metadataRadio.Checked += (_, _) => identity.SetMetadataOnly(true);

        // Auto-first, like the import picker: the pipeline status, its signal
        // badges, and the live match list sit above the manual-search fallback.
        var form = new StackPanel { Spacing = 8, Width = 420 };
        form.Children.Add(pipelineStatus);
        form.Children.Add(badgeHost);
        form.Children.Add(resultsList);
        form.Children.Add(claimRow);
        form.Children.Add(artistBox);
        form.Children.Add(albumBox);
        form.Children.Add(sourceBox);
        form.Children.Add(searchButton);
        form.Children.Add(status);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("album.reidentify.title"),
            Content = new ScrollViewer { Content = form },
            PrimaryButtonText = Loc.Chrome("album.reidentify.confirm"),
            SecondaryButtonText = Loc.Chrome("identify.skip"),
            CloseButtonText = Loc.Chrome("action.cancel"),
            XamlRoot = _xamlRoot(),
            IsPrimaryButtonEnabled = false,
        };

        var candidates = new List<ReleaseCandidateChoice>();

        // Arbitrates who owns the match list: the auto-identify pipeline until a
        // manual search takes it over, and a change-gate so a pipeline refresh
        // that replays the same matches doesn't rebuild the list (which would
        // drop the user's current pick).
        var results = new ReidentifyResultsModel();

        // A signal toggle / re-run asks the pipeline for results again, so it
        // reclaims the list on its next refresh. The re-derived state arrives
        // through the same candidate invalidation the refresh reads.
        void ToggleSignal(string kind, string value)
        {
            results.ResumePipeline();
            _ = _session.RunForCurrentHandle(
                handle => NativeBae.ToggleSignalForCandidate(handle, key, kind, value));
        }

        void Rerun()
        {
            results.ResumePipeline();
            _ = _session.RunForCurrentHandle(
                handle => NativeBae.RerunIdentifyForCandidate(handle, key));
        }

        // Re-read the pipeline snapshot for this key and render its status line,
        // signal badges, and (when it owns the list) match list. Invalidations
        // for other candidates also land here — the registry dispatches by kind,
        // not key — so a missing snapshot for our key is a no-op.
        void RefreshFromPipeline()
        {
            var (current, snapshot) = _session.WithCurrentHandle(
                handle => NativeBae.ReidentifyPipeline(handle, key));
            if (!current || snapshot is null)
            {
                return;
            }

            var (rowStatus, matches, signals) = snapshot.Value;
            pipelineStatus.Text = rowStatus.LocalizedLine;
            pipelineStatus.Visibility = string.IsNullOrEmpty(rowStatus.LocalizedLine)
                ? Visibility.Collapsed : Visibility.Visible;
            badgeHost.Children.Clear();
            if (signals.Count > 0)
            {
                badgeHost.Children.Add(SignalBadgeRow.Build(signals, ToggleSignal, Rerun));
            }
            if (results.ApplyPipelineMatches(matches.Select(match => match.ReleaseId).ToList()))
            {
                candidates = matches;
                resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
            }
        }

        // Start the pipeline against the release's own files; its events flow
        // through the candidate invalidation the registration below reads.
        _session.WithCurrentHandle(handle => NativeBae.AutoIdentifyRelease(handle, key, releaseId));

        // The generated bridge search and commit both block on network/DB work; run them off
        // the UI thread so the dialog stays responsive.
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

            // A successful manual search takes the results list over; later
            // pipeline refreshes keep the status line and badges live but leave
            // these results in place.
            results.ApplyManualResults();
            candidates = search.Candidates ?? [];
            resultsList.ItemsSource = candidates.Select(candidate => candidate.Summary).ToList();
            status.Text = Loc.Chrome("search.no_matches");
            status.Visibility = candidates.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
            dialog.IsPrimaryButtonEnabled = false;
        };

        resultsList.SelectionChanged += (_, _) =>
        {
            var selected = resultsList.SelectedIndex >= 0;
            dialog.IsPrimaryButtonEnabled = selected;
            claimRow.Visibility = selected ? Visibility.Visible : Visibility.Collapsed;
            // Each new pick resets the claim to exact.
            identity = new ImportIdentityModel(hasSourceRelease: true);
            exactRadio.IsChecked = true;
        };

        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var index = resultsList.SelectedIndex;
            if (index < 0 || index >= candidates.Count)
            {
                args.Cancel = true;
                return;
            }

            var chosen = candidates[index];
            var deferral = args.GetDeferral();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ReidentifyRelease(handle, releaseId,
                    NativeBae.SourceIdentityChoice(!identity.MetadataOnly, chosen.ReleaseId, chosen.Source)));
            if (!current)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        // "Skip identifying" clears any source identity in one click: core reseeds
        // the release's rows from the rip's file tags, so there is no editable seed
        // page and no follow-up refresh prompt.
        dialog.SecondaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var (current, error) = await _session.RunForCurrentHandle(
                handle => NativeBae.ReidentifyRelease(handle, releaseId, new BridgeIdentityChoice.Unknown()));
            if (!current)
            {
                args.Cancel = true;
                deferral.Complete();
                return;
            }
            if (error is not null)
            {
                status.Text = error;
                status.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        // The pipeline emits a candidate invalidation for every identify/signals
        // event; refresh from each while the dialog is open, disposing the
        // registration when it closes.
        var registration = _projections.Register(
            typeof(BridgeInvalidation.ImportCandidate), RefreshFromPipeline);
        ContentDialogResult result;
        try
        {
            result = await dialog.ShowAsync();
        }
        finally
        {
            registration.Dispose();
            // Stop the identify driver and any in-flight artwork OCR for this
            // candidate; the dialog is the only consumer of its results.
            _session.WithCurrentHandle(handle => NativeBae.CancelAutoIdentify(handle, key));
        }

        // A Primary result means a source-backed commit succeeded (the error
        // paths cancel the close). Offer to reseed the metadata from the newly
        // pointed source. The Secondary (Unknown) commit reseeds rows from the
        // rip's file tags itself, so it has nothing to confirm.
        if (result == ContentDialogResult.Primary)
        {
            await ShowRefreshPrompt(releaseId);
        }
    }

    // After a source-backed identity commit, offer to reseed the release's
    // metadata from the newly-pointed source. Refresh overwrites the user's
    // prior edits by design; Keep leaves the stored metadata as-is. The identity
    // itself is already committed either way.
    private async System.Threading.Tasks.Task ShowRefreshPrompt(string releaseId)
    {
        var body = new TextBlock
        {
            Text = Loc.Chrome("album.reidentify.refresh_body"),
            TextWrapping = TextWrapping.Wrap,
        };
        var error = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Salmon),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var content = new StackPanel { Spacing = 8, Width = 420 };
        content.Children.Add(body);
        content.Children.Add(error);

        var dialog = new ContentDialog
        {
            Title = Loc.Chrome("album.reidentify.updated"),
            Content = content,
            PrimaryButtonText = Loc.Chrome("album.reidentify.refresh_confirm"),
            CloseButtonText = Loc.Chrome("album.reidentify.keep_current"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = _xamlRoot(),
        };

        // Refresh runs off the UI thread behind a deferral. On error keep the
        // prompt open with the reason shown — the identity commit already
        // succeeded, so the user can retry Refresh or fall back to Keep.
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            var (current, refreshError) = await _session.RunForCurrentHandle(
                handle => NativeBae.RefreshMetadataFromSource(handle, releaseId));
            if (!current)
            {
                deferral.Complete();
                return;
            }
            if (refreshError is not null)
            {
                error.Text = refreshError;
                error.Visibility = Visibility.Visible;
                args.Cancel = true;
            }

            deferral.Complete();
        };

        await dialog.ShowAsync();
    }
}
