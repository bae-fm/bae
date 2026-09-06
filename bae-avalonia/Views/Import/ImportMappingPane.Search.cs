using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Layout;
using uniffi.bae_bridge;

namespace Bae.Desktop;

internal sealed partial class ImportMappingPane
{
    // Find online is one page: the result area on top, the typed search form
    // docked below it. The identify verdict and a submitted search share that
    // result area — a search takes it over while it is running, and clearing
    // the search gives it back.
    private Control BuildSearchEditor()
    {
        var column = new StackPanel { Spacing = 8 };
        column.Children.Add(SearchResultArea());
        column.Children.Add(SearchForm());
        return column;
    }

    private Control SearchResultArea() =>
        _runtime?.Search is { } search ? SearchRunArea(search) : IdentifyArea();

    // What identification has to say: its badge row, the pressings it found,
    // and the sources that failed. A failed identify still lists what the
    // provider that did answer found.
    private Control IdentifyArea()
    {
        var column = new StackPanel { Spacing = 8 };
        var signals = _import.ProjectRun(_runtime).Signals;
        if (signals.Count > 0 && _key is { } signalKey)
        {
            column.Children.Add(SignalBadgeRow.Build(
                signals,
                (kind, value) => _ = _app.Import.ToggleSignalForCandidate(
                    signalKey, kind, value),
                () => _ = _app.Import.RerunIdentifyForCandidate(signalKey)));
        }
        var matches = EffectiveMatches;
        if (matches.Count > 0)
        {
            column.Children.Add(ChoiceList(matches));
        }
        if (ShownIdentifyState is BridgeIdentifyState.Failed failed)
        {
            foreach (var failure in failed.Failures)
            {
                column.Children.Add(ImportPaneUi.Cell(
                    BridgeDisplay.LocalizedLine(failure),
                    secondary: true));
            }
        }
        else if (matches.Count == 0
            && ShownIdentifyState is not BridgeIdentifyState.Idle
            and not BridgeIdentifyState.Triangulating)
        {
            column.Children.Add(ImportPaneUi.Cell(
                Loc.Chrome("import.search.no_automatic_matches"),
                secondary: true));
        }
        if (ShownIdentifyState is BridgeIdentifyState.Triangulating
            or BridgeIdentifyState.Idle)
        {
            column.Children.Add(new Spinner { Width = 16, Height = 16 });
        }
        else if (signals.Count == 0)
        {
            var rerun = ImportPaneUi.RowButton(Loc.Chrome("import.rerun_identify"));
            rerun.Click += (_, _) =>
            {
                if (_key is { } key)
                {
                    _ = _app.Import.RerunIdentifyForCandidate(key);
                }
            };
            column.Children.Add(rerun);
        }
        return column;
    }

    // A submitted search, as its providers land. Whatever has answered draws
    // straight away; a provider still looking says so, and one that failed says
    // so with its own Retry.
    private Control SearchRunArea(BridgeCandidateSearch search)
    {
        var column = new StackPanel { Spacing = 8 };

        var header = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        header.Children.Add(ImportPaneUi.Cell(
            Loc.Chrome("import.search.results_for", "query", SearchQuerySummary(search.Query)),
            secondary: true));
        var clear = ImportPaneUi.RowButton(Loc.Chrome("import.search.identification_results"));
        clear.Click += (_, _) =>
        {
            if (_key is { } key)
            {
                _import.ClearCandidateSearch(key);
            }
        };
        header.Children.Add(clear);
        column.Children.Add(header);

        var choices = _app.Import.GroupChoices(search.Groups);
        if (choices.Count > 0)
        {
            column.Children.Add(ChoiceList(choices));
        }

        foreach (var (source, state) in new[]
        {
            (BridgeMetadataSource.MusicBrainz, search.Musicbrainz),
            (BridgeMetadataSource.Discogs, search.Discogs),
        })
        {
            switch (state)
            {
                case BridgeSourceSearch.Searching:
                    column.Children.Add(ImportPaneUi.Cell(
                        Loc.Chrome(
                            "import.search.searching_source",
                            "source",
                            BaeBridgeMethods.BridgeMetadataSourceName(source)),
                        secondary: true));
                    break;
                case BridgeSourceSearch.Failed failure:
                    column.Children.Add(SourceFailureRow(source, failure.Failure));
                    break;
                case BridgeSourceSearch.NotConfigured:
                    column.Children.Add(ImportPaneUi.Cell(
                        Loc.Chrome(
                            "import.search.source_not_configured",
                            "source",
                            BaeBridgeMethods.BridgeMetadataSourceName(source)),
                        secondary: true));
                    break;
            }
        }

        if (search.NoMatches)
        {
            column.Children.Add(ImportPaneUi.Cell(
                Loc.Chrome("search.no_matches"), secondary: true));
        }
        return column;
    }

    private Control SourceFailureRow(
        BridgeMetadataSource source,
        BridgeLookupFailure failure)
    {
        var row = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        row.Children.Add(ImportPaneUi.Cell(
            Loc.Chrome(
                "import.search.source_failed",
                new Dictionary<string, object?>
                {
                    ["source"] = BaeBridgeMethods.BridgeMetadataSourceName(source),
                    ["reason"] = BridgeDisplay.LocalizedLine(failure),
                }),
            secondary: true));
        var retry = ImportPaneUi.RowButton(Loc.Chrome("action.retry"));
        retry.Click += (_, _) =>
        {
            if (_key is { } key)
            {
                _import.RetryCandidateSearch(key);
            }
        };
        row.Children.Add(retry);
        return row;
    }

    /// <summary>What the search asked for, as the header line's subject.</summary>
    private static string SearchQuerySummary(BridgeSearchQuery query) => query switch
    {
        BridgeSearchQuery.General general => string.Join(
            " · ",
            new[] { general.Artist, general.Album }
                .Where(part => !string.IsNullOrWhiteSpace(part))),
        BridgeSearchQuery.CatalogNumber catalog => catalog.CatalogNumberValue,
        BridgeSearchQuery.Barcode barcode => barcode.BarcodeValue,
        _ => throw new ArgumentOutOfRangeException(
            nameof(query), query, "Unknown search query"),
    };

    // The typed form, docked below the result area whatever the result area is
    // showing. No source selection: every configured provider is asked.
    private Control SearchForm()
    {
        var column = new StackPanel { Spacing = 8 };

        var types = new StackPanel
        {
            Orientation = Orientation.Horizontal,
            Spacing = 8,
        };
        types.Children.Add(ManualSearchTypeButton(
            Loc.Chrome("import.search.general"),
            ManualSearchType.General));
        types.Children.Add(ManualSearchTypeButton(
            Loc.Chrome("signal.kind.catalog"),
            ManualSearchType.Catalog));
        types.Children.Add(ManualSearchTypeButton(
            Loc.Chrome("signal.kind.barcode"),
            ManualSearchType.Barcode));
        column.Children.Add(types);

        Control fields = _manualSearchType switch
        {
            ManualSearchType.General => GeneralSearchFields(),
            ManualSearchType.Catalog => SearchValueField(
                Loc.Chrome("signal.kind.catalog"),
                _searchCatalog,
                value => _searchCatalog = value),
            ManualSearchType.Barcode => SearchValueField(
                Loc.Chrome("signal.kind.barcode"),
                _searchBarcode,
                value => _searchBarcode = value),
            _ => throw new ArgumentOutOfRangeException(
                nameof(_manualSearchType), _manualSearchType, "Unknown manual search type"),
        };
        column.Children.Add(fields);

        var search = ImportPaneUi.RowButton(Loc.Chrome("action.search"));
        search.Click += (_, _) =>
        {
            if (_key is { } key)
            {
                _import.StartCandidateSearch(key, ManualSearchQuery());
            }
        };

        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
        actions.Children.Add(search);
        column.Children.Add(actions);
        return column;
    }

    private Button ManualSearchTypeButton(
        string label,
        ManualSearchType type)
    {
        var button = ImportPaneUi.RowButton(label);
        button.IsEnabled = _manualSearchType != type;
        button.Click += (_, _) =>
        {
            _manualSearchType = type;
            Render();
        };
        return button;
    }

    private Control GeneralSearchFields()
    {
        var artistField = DialogUi.Field(Loc.Chrome("import.field.artist_manual"), out var artistBox);
        artistBox.Text = _searchArtist;
        artistBox.TextChanged += (_, _) => _searchArtist = artistBox.Text ?? string.Empty;
        var albumField = DialogUi.Field(Loc.Chrome("search.field.album"), out var albumBox);
        albumBox.Text = _searchAlbum;
        albumBox.TextChanged += (_, _) => _searchAlbum = albumBox.Text ?? string.Empty;
        var fields = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,*"),
            ColumnSpacing = 8,
        };
        Grid.SetColumn(artistField, 0);
        Grid.SetColumn(albumField, 1);
        fields.Children.Add(artistField);
        fields.Children.Add(albumField);
        return fields;
    }

    private static Control SearchValueField(
        string label,
        string value,
        Action<string> onChange)
    {
        var valueField = DialogUi.Field(label, out var valueBox);
        valueBox.Text = value;
        valueBox.TextChanged += (_, _) => onChange(valueBox.Text ?? string.Empty);
        return valueField;
    }

    private BridgeSearchQuery ManualSearchQuery() =>
        _manualSearchType switch
        {
            ManualSearchType.General => new BridgeSearchQuery.General(
                _searchArtist,
                _searchAlbum),
            ManualSearchType.Catalog => new BridgeSearchQuery.CatalogNumber(_searchCatalog),
            ManualSearchType.Barcode => new BridgeSearchQuery.Barcode(_searchBarcode),
            _ => throw new ArgumentOutOfRangeException(
                nameof(_manualSearchType), _manualSearchType, "Unknown manual search type"),
        };

    // The pressings on offer, whichever half produced them. Picking one
    // applies that external release, and the editor closes only after the
    // candidate subscription delivers that provenance too.
    private Control ChoiceList(List<ReleaseCandidateChoice> choices)
    {
        var results = new ListBox { SelectionMode = SelectionMode.Single, MaxHeight = 190 };
        results.ItemsSource = choices.Select(ChoiceRow).ToList();
        results.IsEnabled = _applyingProvenance is null;
        results.SelectionChanged += async (_, _) =>
        {
            if (results.SelectedIndex < 0 || results.SelectedIndex >= choices.Count)
            {
                return;
            }
            var chosen = choices[results.SelectedIndex];
            await ApplyMetadata(chosen.Provenance);
        };
        return results;
    }

    private Control ChoiceRow(ReleaseCandidateChoice choice)
    {
        var row = new Grid
        {
            ColumnDefinitions = new ColumnDefinitions("*,Auto"),
            ColumnSpacing = 8,
            Margin = new Thickness(4, 6),
        };
        row.Children.Add(ImportPaneUi.Cell(choice.Summary));
        var progress = new Spinner
        {
            Width = 14,
            Height = 14,
            IsVisible = _applyingProvenance is BridgeMetadataProvenance.ExternalRelease applying
                && applying.Source == choice.Source
                && applying.ReleaseId == choice.ReleaseId,
        };
        Grid.SetColumn(progress, 1);
        row.Children.Add(progress);
        return row;
    }
}
