using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ArtistAssignmentsFieldTests
{
    [AvaloniaFact]
    public void ChoosingASearchResultKeepsTheExistingArtistIdentity()
    {
        var existing = new BridgeExistingArtist(
            "artist-1",
            "Artist Name",
            "Name, Artist",
            "mb-artist-1",
            null);
        IReadOnlyList<BridgeArtistAssignment>? written = null;
        var field = Attach(new ArtistAssignmentsField(
            Array.Empty<BridgeArtistAssignment>(),
            new LibraryService
            {
                SearchArtists = _ => Task.FromResult((
                    true,
                    ((List<BridgeArtistSearchResult>?)
                        [new BridgeArtistSearchResult(existing, null)],
                        (string?)null))),
            },
            assignments => written = assignments));

        Open(field);
        Query(field).Text = "Artist";
        Click(field, Loc.Chrome("action.search"));
        Click(field, "Artist Name");

        var selected = Assert.Single(Assert.IsAssignableFrom<
            IReadOnlyList<BridgeArtistAssignment>>(written));
        Assert.Equal(existing, Assert.IsType<BridgeArtistAssignment.Existing>(selected).Artist);
    }

    [AvaloniaFact]
    public void AddingTypedTextCreatesANewArtistSeed()
    {
        IReadOnlyList<BridgeArtistAssignment>? written = null;
        var field = Attach(new ArtistAssignmentsField(
            Array.Empty<BridgeArtistAssignment>(),
            new LibraryService(),
            assignments => written = assignments));

        Open(field);
        Query(field).Text = "New Artist";
        Click(field, Loc.Chrome("artist.assignments.add"));

        var selected = Assert.Single(Assert.IsAssignableFrom<
            IReadOnlyList<BridgeArtistAssignment>>(written));
        var created = Assert.IsType<BridgeArtistAssignment.New>(selected);
        Assert.Equal("New Artist", created.Seed.Name);
    }

    private static ArtistAssignmentsField Attach(ArtistAssignmentsField field)
    {
        var window = new Window { Width = 500, Height = 300, Content = field };
        window.Show();
        return field;
    }

    private static void Open(ArtistAssignmentsField field) =>
        Assert.IsType<Button>(field.Content)
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static TextBox Query(ArtistAssignmentsField field) =>
        Assert.Single(Editor(field).GetLogicalDescendants().OfType<TextBox>());

    private static void Click(ArtistAssignmentsField field, string label) =>
        Assert.Single(
            Editor(field).GetLogicalDescendants().OfType<Button>(),
            button => button.Content switch
            {
                string text => text == label,
                StackPanel summary => summary.Children
                    .OfType<TextBlock>()
                    .Any(text => text.Text == label),
                _ => false,
            })
            .RaiseEvent(new RoutedEventArgs(Button.ClickEvent));

    private static Control Editor(ArtistAssignmentsField field) =>
        Assert.IsType<Flyout>(Assert.IsType<Button>(field.Content).Flyout).Content
            as Control
        ?? throw new InvalidOperationException("artist editor did not build a control");
}
