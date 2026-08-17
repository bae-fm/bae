using System.Collections.Generic;
using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the activation-intent grammar: the folder-argument rule (its own
/// rooted-path check, not System.IO.Path — this test project also runs on the
/// macOS host, where Path's rootedness semantics differ), the file:// form a
/// file manager hands over for a folder, the bae://import URI form, and the argv
/// tokenizer for an activation delivered as one raw command line. Every path
/// shape is accepted wherever the tests run, so the same cases hold on the macOS
/// host and on both shipping desktops. Filesystem access is stubbed through
/// isDirectory; the handler registration and the window plumbing that produce
/// and dispatch an intent live outside this pure model and are verified by
/// compilation.
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

    [Fact]
    public void PosixRootedDirectory_ReturnsImportFolder()
    {
        var intent = ActivationIntentModel.Parse(
            Args("/home/listener/Music"), Dirs("/home/listener/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/Music"), intent);
    }

    [Fact]
    public void PosixRootedPathThatIsNotADirectory_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args("/home/listener/Music"), Dirs());
        Assert.Null(intent);
    }

    // A path is judged by its own shape, never round-tripped through URI parsing:
    // the folder handed to the import has to be the argument as it was given.
    [Fact]
    public void PathWithPercentSequenceInItsName_IsNotDecoded()
    {
        var intent = ActivationIntentModel.Parse(
            Args("/home/listener/My%20Music"), Dirs("/home/listener/My%20Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/My%20Music"), intent);
    }

    // --- file:// folder argument -------------------------------------------

    [Fact]
    public void FileUri_ReturnsImportFolder()
    {
        var intent = ActivationIntentModel.Parse(
            Args("file:///home/listener/Music"), Dirs("/home/listener/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/Music"), intent);
    }

    [Fact]
    public void FileUriWithPercentEncodedSpace_Decodes()
    {
        var intent = ActivationIntentModel.Parse(
            Args("file:///home/listener/My%20Music"), Dirs("/home/listener/My Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/My Music"), intent);
    }

    [Fact]
    public void FileUriWithLocalhostHost_ReturnsImportFolder()
    {
        var intent = ActivationIntentModel.Parse(
            Args("file://localhost/home/listener/Music"), Dirs("/home/listener/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/Music"), intent);
    }

    [Fact]
    public void FileUriWithRemoteHost_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args("file://storage/share/Music"), path => true);
        Assert.Null(intent);
    }

    [Fact]
    public void FileUriWithDriveLetter_DropsTheLeadingSlash()
    {
        var intent = ActivationIntentModel.Parse(Args("file:///C:/Music"), Dirs("C:/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("C:/Music"), intent);
    }

    [Fact]
    public void FileUriWithPercentEncodedDriveColon_DropsTheLeadingSlash()
    {
        var intent = ActivationIntentModel.Parse(Args("file:///C%3A/Music"), Dirs("C:/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("C:/Music"), intent);
    }

    [Fact]
    public void FileUriThatIsNotADirectory_ReturnsNull()
    {
        var intent = ActivationIntentModel.Parse(Args("file:///home/listener/track.flac"), Dirs());
        Assert.Null(intent);
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

    // A leading dash is no rooted shape, whatever the directory check would say —
    // the accepted POSIX form starts with a slash and nothing else does.
    [Theory]
    [InlineData("--veloapp-install")]
    [InlineData("--veloapp-updated")]
    [InlineData("-v")]
    [InlineData("--capture-shots")]
    public void Flags_AreNeverAPath(string arg)
    {
        var intent = ActivationIntentModel.Parse(Args(arg), path => true);
        Assert.Null(intent);
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
    public void PosixPathInTheQuery_ReturnsImportFolder()
    {
        var intent = ActivationIntentModel.Parse(
            Args("bae://import?path=/home/listener/Music"), Dirs("/home/listener/Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/Music"), intent);
    }

    [Fact]
    public void PercentEncodedPosixPathInTheQuery_Decodes()
    {
        var intent = ActivationIntentModel.Parse(
            Args("bae://import?path=%2Fhome%2Flistener%2FMy%20Music"), Dirs("/home/listener/My Music"));
        Assert.Equal(new ActivationIntent.ImportFolder("/home/listener/My Music"), intent);
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
    [InlineData("bae://unknown?value=x")]
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
}
