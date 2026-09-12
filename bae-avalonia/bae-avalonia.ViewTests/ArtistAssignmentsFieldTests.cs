using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Headless.XUnit;
using Avalonia.Interactivity;
using Avalonia.LogicalTree;
using Avalonia.Threading;
using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

public sealed class ArtistAssignmentsFieldTests
{
    [AvaloniaFact]
    public void LinkedAndNewAssignmentsRemainVisiblyDistinct()
    {
        var field = Attach(new ArtistAssignmentsField(
            [
                new BridgeArtistAssignment.Existing(new BridgeExistingArtist(
                    "artist-1", "Artist Name", null, null, null)),
                new BridgeArtistAssignment.New(new BridgeNewArtistSeed(
                    "Artist Name", null, null, null)),
            ],
            new LibraryService(),
            _ => { }));

        Open(field);

        var labels = Editor(field).GetLogicalDescendants()
            .OfType<TextBlock>()
            .Select(text => text.Text)
            .ToList();
        Assert.Contains(Loc.Chrome("artist.assignments.library"), labels);
        Assert.Contains(Loc.Chrome("artist.assignments.new"), labels);
    }

    [AvaloniaFact]
    public void SameNameSearchResultsExposeTheirExactLibraryIdentity()
    {
        var first = new BridgeExistingArtist(
            "artist-1", "Artist Name", "Name, Artist", null, null);
        var second = new BridgeExistingArtist(
            "artist-2", "Artist Name", "Name, Artist", null, null);
        IReadOnlyList<BridgeArtistAssignment>? written = null;
        var field = Attach(new ArtistAssignmentsField(
            Array.Empty<BridgeArtistAssignment>(),
            new LibraryService
            {
                SearchArtists = _ => Task.FromResult((
                    true,
                    ((List<BridgeArtistSearchResult>?)
                        [new(first, null), new(second, null)],
                        (string?)null))),
            },
            assignments => written = assignments));

        Open(field);
        Query(field).Text = "Artist";
        Click(field, Loc.Chrome("action.search"));

        var details = Editor(field).GetLogicalDescendants()
            .OfType<TextBlock>()
            .Select(text => text.Text)
            .ToList();
        Assert.Contains(details, text => text?.Contains("artist-1") == true);
        Assert.Contains(details, text => text?.Contains("artist-2") == true);

        Click(field, "artist-2");
        var selected = Assert.Single(Assert.IsAssignableFrom<
            IReadOnlyList<BridgeArtistAssignment>>(written));
        Assert.Equal(
            "artist-2",
            Assert.IsType<BridgeArtistAssignment.Existing>(selected).Artist.ArtistId);
    }

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

    [AvaloniaFact]
    public void OneLibraryArtistSummarizesToItsOwnNameAndBadge()
    {
        var summary = ArtistAssignmentDisplay.Summarize(
            [LibraryArtist("artist-1", "Artist Name")]);

        Assert.NotNull(summary);
        Assert.Equal("Artist Name", summary!.Value.Names);
        Assert.Equal(Loc.Chrome("artist.assignments.library"), summary.Value.Identity);
    }

    [AvaloniaFact]
    public void OneNewArtistSummarizesToItsOwnNameAndBadge()
    {
        var summary = ArtistAssignmentDisplay.Summarize(
            [NewArtist("New Artist Name")]);

        Assert.NotNull(summary);
        Assert.Equal("New Artist Name", summary!.Value.Names);
        Assert.Equal(Loc.Chrome("artist.assignments.new"), summary.Value.Identity);
    }

    [AvaloniaFact]
    public void ArtistsAllInTheLibraryCarryOneLibraryBadge()
    {
        var assignments = new List<BridgeArtistAssignment>
        {
            LibraryArtist("artist-1", "First Artist"),
            LibraryArtist("artist-2", "Second Artist"),
            LibraryArtist("artist-3", "Third Artist"),
        };

        var summary = ArtistAssignmentDisplay.Summarize(assignments);

        Assert.NotNull(summary);
        Assert.Equal(ArtistAssignmentDisplay.Join(assignments), summary!.Value.Names);
        Assert.Equal(Loc.Chrome("artist.assignments.library"), summary.Value.Identity);
    }

    [AvaloniaFact]
    public void ArtistsAllNewToTheLibraryCarryOneNewBadge()
    {
        var assignments = new List<BridgeArtistAssignment>
        {
            NewArtist("First Artist"),
            NewArtist("Second Artist"),
        };

        var summary = ArtistAssignmentDisplay.Summarize(assignments);

        Assert.NotNull(summary);
        Assert.Equal(ArtistAssignmentDisplay.Join(assignments), summary!.Value.Names);
        Assert.Equal(Loc.Chrome("artist.assignments.new"), summary.Value.Identity);
    }

    [AvaloniaFact]
    public void AMixedSetCountsTheArtistsNewToTheLibrary()
    {
        var assignments = new List<BridgeArtistAssignment>
        {
            LibraryArtist("artist-1", "First Artist"),
            NewArtist("Second Artist"),
            LibraryArtist("artist-3", "Third Artist"),
            NewArtist("Fourth Artist"),
        };

        var summary = ArtistAssignmentDisplay.Summarize(assignments);

        Assert.NotNull(summary);
        Assert.Equal(ArtistAssignmentDisplay.Join(assignments), summary!.Value.Names);
        Assert.Equal(
            Loc.Chrome("artist.assignments.new_count", "count", 2),
            summary.Value.Identity);
    }

    /// <summary>A compilation credits more artists than the field can draw. The
    /// closed field summarizes them into the width it has rather than running
    /// past its container, and the badge stays in view.</summary>
    [AvaloniaFact]
    public void ACompilationsArtistsStayInsideTheFieldsContainer()
    {
        var assignments = Enumerable.Range(1, 9)
            .Select(index => LibraryArtist($"artist-{index}", $"Artist Name {index}"))
            .Concat(new[]
            {
                NewArtist("New Artist One"),
                NewArtist("New Artist Two"),
                NewArtist("New Artist Three"),
            })
            .ToList();
        var field = new ArtistAssignmentsField(
            assignments,
            new LibraryService(),
            _ => { });
        var container = new Border { Width = 600, Child = field };
        var window = new Window { Width = 900, Height = 300, Content = container };
        window.Show();
        try
        {
            Dispatcher.UIThread.RunJobs();
            window.Measure(new Size(900, 300));
            window.Arrange(new Rect(0, 0, 900, 300));
            Dispatcher.UIThread.RunJobs();

            Assert.True(field.DesiredSize.Width <= 600);
            var badge = Loc.Chrome("artist.assignments.new_count", "count", 3);
            var texts = field.GetLogicalDescendants().OfType<TextBlock>().ToList();
            Assert.Contains(texts, text => text.Text == badge);
            foreach (var text in texts)
            {
                var origin = text.TranslatePoint(default, field)!.Value;
                Assert.True(origin.X >= -0.5);
                Assert.True(origin.X + text.Bounds.Width <= field.Bounds.Width + 0.5);
            }
        }
        finally { window.Close(); }
    }

    private static BridgeArtistAssignment LibraryArtist(string artistId, string name) =>
        new BridgeArtistAssignment.Existing(
            new BridgeExistingArtist(artistId, name, null, null, null));

    private static BridgeArtistAssignment NewArtist(string name) =>
        new BridgeArtistAssignment.New(new BridgeNewArtistSeed(name, null, null, null));

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
