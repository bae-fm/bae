using System.Collections.Generic;
using Bae.Windows;
using Xunit;

namespace Bae.Windows.Tests;

/// <summary>
/// Locks the activation-intent grammar: the folder-verb / dropped-folder argv
/// rule (its own Windows-rooted-path check, not System.IO.Path — this test
/// project also runs on the macOS host, where Path's rootedness semantics
/// differ), the bae://import URI form, and the argv tokenizer for a
/// redirected activation's raw command line. Filesystem access is stubbed
/// through isDirectory; the registry writes and AppInstance plumbing that
/// produce and dispatch an intent live outside this pure model and are
/// verified by compilation.
/// </summary>
public sealed class ActivationIntentModelTests
{
    private static System.Func<string, bool> Dirs(params string[] directories)
    {
        var set = new HashSet<string>(directories);
        return path => set.Contains(path);
    }

    private static IReadOnlyList<string> Args(params string[] args) => args;

    // --- Folder argument -----------------------------------------------

    [Fact]
    public void RootedExistingDirectory_ReturnsImportFolder()
    {
        var intent = ActivationIntentModel.Parse(Args(@"C:\Music"), Dirs(@"C:\Music"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\Music"), intent);
    }

    [Fact]
    public void RootedPathThatIsNotADirectory_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args(@"C:\Music"), Dirs());
        Assert.Null(intent);
    }

    [Fact]
    public void RelativePath_ReturnsNull_EvenWhenItWouldBeADirectory()
    {
        var intent = ActivationIntentModel.Parse(Args("Music"), path => true);
        Assert.Null(intent);
    }

    [Fact]
    public void UncPathDirectory_ReturnsImportFolder()
    {
        var intent = ActivationIntentModel.Parse(
            Args(@"\\storage\share\Music"), Dirs(@"\\storage\share\Music"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"\\storage\share\Music"), intent);
    }

    // --- Argument scan ----------------------------------------------------

    [Fact]
    public void LeadingExeToken_IsSkipped_NotADirectory()
    {
        var intent = ActivationIntentModel.Parse(
            Args(@"C:\Program Files\bae\bae.exe", @"C:\Music"), Dirs(@"C:\Music"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\Music"), intent);
    }

    [Fact]
    public void VeloappFlags_AreSkipped_NotARootedPath()
    {
        var intent = ActivationIntentModel.Parse(
            Args("--veloapp-updated", "1.2.3", @"C:\Music"), Dirs(@"C:\Music"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\Music"), intent);
    }

    [Fact]
    public void FirstMatchingArgument_Wins()
    {
        var intent = ActivationIntentModel.Parse(
            Args(@"C:\First", @"C:\Second"), Dirs(@"C:\First", @"C:\Second"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\First"), intent);
    }

    // --- bae://import?path=... ---------------------------------------------

    [Fact]
    public void PercentEncodedBackslashPath_Decodes()
    {
        var intent = ActivationIntentModel.Parse(
            Args("bae://import?path=C%3A%5CUsers%5Cme%5CMusic"), Dirs(@"C:\Users\me\Music"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\Users\me\Music"), intent);
    }

    [Fact]
    public void PercentEncodedForwardSlashPath_Decodes()
    {
        var intent = ActivationIntentModel.Parse(
            Args("bae://import?path=C%3A/Users/me/Music"), Dirs("C:/Users/me/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("C:/Users/me/Music"), intent);
    }

    [Fact]
    public void SchemeAndHost_AreCaseInsensitive()
    {
        var intent = ActivationIntentModel.Parse(
            Args("BAE://IMPORT?path=C%3A%5CMusic"), Dirs(@"C:\Music"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\Music"), intent);
    }

    [Fact]
    public void MissingPathParam_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args("bae://import"), path => true);
        Assert.Null(intent);
    }

    [Fact]
    public void EmptyPathParam_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args("bae://import?path="), path => true);
        Assert.Null(intent);
    }

    [Fact]
    public void DecodedPathNotADirectory_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args("bae://import?path=C%3A%5CMusic"), Dirs());
        Assert.Null(intent);
    }

    [Fact]
    public void ExtraQueryParams_AreIgnored_FirstPathWins()
    {
        var intent = ActivationIntentModel.Parse(
            Args("bae://import?other=1&path=C%3A%5CFirst&path=C%3A%5CSecond"),
            Dirs(@"C:\First", @"C:\Second"));
        Assert.Equal(new ActivationIntent.ImportFolder(@"C:\First"), intent);
    }

    // --- Unknown hosts / paths -> null --------------------------------------

    [Theory]
    [InlineData("bae://album/anything")]
    [InlineData("bae://invite?code=x")]
    [InlineData("bae://")]
    [InlineData("bae:import")]
    public void UnknownHostsAndPaths_ReturnNull(string arg)
    {
        var intent = ActivationIntentModel.Parse(Args(arg), path => true);
        Assert.Null(intent);
    }

    // --- Garbage -> null -----------------------------------------------------

    [Fact]
    public void EmptyArgs_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args(), path => true);
        Assert.Null(intent);
    }

    [Theory]
    [InlineData(" ")]
    [InlineData("not a uri")]
    [InlineData("http://example.com")]
    [InlineData("bae://[invalid")]
    public void GarbageArguments_ReturnNull(string arg)
    {
        var intent = ActivationIntentModel.Parse(Args(arg), path => true);
        Assert.Null(intent);
    }

    // --- SplitCommandLine ------------------------------------------------

    [Fact]
    public void SplitCommandLine_EmptyString_ReturnsEmptyList()
    {
        Assert.Empty(ActivationIntentModel.SplitCommandLine(string.Empty));
    }

    [Fact]
    public void SplitCommandLine_UnquotedTokens_SplitOnWhitespace()
    {
        var tokens = ActivationIntentModel.SplitCommandLine("bae.exe --veloapp-updated 1.2.3");
        Assert.Equal(new[] { "bae.exe", "--veloapp-updated", "1.2.3" }, tokens);
    }

    [Fact]
    public void SplitCommandLine_QuotedPathWithSpaces_RoundTrips()
    {
        var tokens = ActivationIntentModel.SplitCommandLine(
            "\"C:\\Program Files\\bae\\bae.exe\" \"C:\\Users\\me\\My Music\"");
        Assert.Equal(new[] { @"C:\Program Files\bae\bae.exe", @"C:\Users\me\My Music" }, tokens);
    }

    [Fact]
    public void SplitCommandLine_EmbeddedEscapedQuote_IsLiteral()
    {
        var tokens = ActivationIntentModel.SplitCommandLine("\"a \\\"quoted\\\" value\"");
        Assert.Equal(new[] { "a \"quoted\" value" }, tokens);
    }

    [Fact]
    public void SplitCommandLine_DoubledTrailingBackslashBeforeClosingQuote_CollapsesAndCloses()
    {
        // A UNC share root ending in one visible backslash is quoted with that
        // backslash doubled (the standard argv-escaping convention): an even
        // backslash run before the quote collapses to half as many literal
        // backslashes and the quote is a real delimiter, not an escape.
        var commandLine = "\"" + @"\\storage\share\\" + "\"";
        var tokens = ActivationIntentModel.SplitCommandLine(commandLine);
        Assert.Equal(new[] { @"\\storage\share\" }, tokens);
    }
}
