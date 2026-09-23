using Bae.Desktop;
using uniffi.bae_bridge;
using Xunit;

namespace Bae.Desktop.ViewTests;

/// <summary>
/// The whole-value edits a badge makes to what a candidate's identification
/// asks about — value in, value out, so they are checked as values.
/// </summary>
public sealed class LookupChoiceEditsTests
{
    private static readonly BridgeLookupChoices Choices = new(
        DiscIdExcluded: false,
        ExcludedBarcodes: ["9999999999999"],
        ChosenCatalogs: ["LBL 001"],
        SearchWords: null,
        DiscountedCatalogs: ["LBL 100"]);

    // A struck-out number is not one the run looks up: striking it out takes
    // it out of the chosen numbers, whatever else stands.
    [Fact]
    public void StrikingANumberOutTakesItOutOfTheChosenOnes()
    {
        var struck = LookupChoiceEdits.Discounting(Choices, "LBL 001", null);

        Assert.Equal(new[] { "LBL 001", "LBL 100" }, struck.DiscountedCatalogs);
        Assert.Empty(struck.ChosenCatalogs);
        Assert.Equal(new[] { "9999999999999" }, struck.ExcludedBarcodes);
        Assert.False(struck.DiscIdExcluded);
    }

    // Counting a number again chooses it only when it is the picked record's
    // own: keeping that agreement is what chose it.
    [Fact]
    public void CountingANumberAgainRechoosesItOnlyForThePickedRecord()
    {
        var picked = LookupChoiceEdits.Discounting(Choices, "LBL 100", "LBL 100");
        Assert.Empty(picked.DiscountedCatalogs);
        Assert.Equal(new[] { "LBL 001", "LBL 100" }, picked.ChosenCatalogs);

        var other = LookupChoiceEdits.Discounting(Choices, "LBL 100", "LBL 200");
        Assert.Empty(other.DiscountedCatalogs);
        Assert.Equal(new[] { "LBL 001" }, other.ChosenCatalogs);

        Assert.Equal(
            new[] { "LBL 001" },
            LookupChoiceEdits.Discounting(Choices, "LBL 100", null).ChosenCatalogs);
    }
}
