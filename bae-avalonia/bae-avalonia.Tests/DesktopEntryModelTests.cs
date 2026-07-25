using Bae.Desktop;
using Xunit;

namespace Bae.Desktop.Tests;

/// <summary>
/// Locks the desktop entry the Linux registration writes: the two handler
/// declarations it makes, and the escaping the specification stacks on the
/// <c>Exec</c> value — a path is quoted whole, the characters that are special
/// inside those quotes take a backslash, and the general string escaping then
/// doubles every backslash that produced. The file write and the
/// desktop-database refresh live outside this pure model.
/// </summary>
public sealed class DesktopEntryModelTests
{
    [Fact]
    public void Build_DeclaresTheSchemeAndFolderHandling()
    {
        var entry = DesktopEntryModel.Build("bae", "/home/listener/Apps/bae.AppImage");

        Assert.Equal(
            "[Desktop Entry]\n"
            + "Type=Application\n"
            + "Version=1.5\n"
            + "Name=bae\n"
            + "Exec=\"/home/listener/Apps/bae.AppImage\" %u\n"
            + "Terminal=false\n"
            + "MimeType=x-scheme-handler/bae;inode/directory;\n",
            entry);
    }

    [Fact]
    public void Build_CarriesTheEditionName()
    {
        var entry = DesktopEntryModel.Build("baeium", "/opt/baeium.AppImage");

        Assert.Contains("Name=baeium\n", entry);
        Assert.Contains("Exec=\"/opt/baeium.AppImage\" %u\n", entry);
    }

    // --- Exec quoting ---------------------------------------------------

    [Fact]
    public void PlainPath_IsQuotedWhole()
    {
        Assert.Equal("\"/opt/bae.AppImage\"", DesktopEntryModel.QuoteExecArgument("/opt/bae.AppImage"));
    }

    [Fact]
    public void Spaces_StayLiteralInsideTheQuotes()
    {
        Assert.Equal(
            "\"/home/listener/My Apps/bae.AppImage\"",
            DesktopEntryModel.QuoteExecArgument("/home/listener/My Apps/bae.AppImage"));
    }

    [Fact]
    public void DollarSign_IsEscapedTwice()
    {
        // The quoting escape produces a backslash, and the string escaping then
        // doubles it — the representation the specification calls unambiguous.
        Assert.Equal("\"/opt/a\\\\$b\"", DesktopEntryModel.QuoteExecArgument("/opt/a$b"));
    }

    [Fact]
    public void Backslash_BecomesFour()
    {
        Assert.Equal("\"/opt/a\\\\\\\\b\"", DesktopEntryModel.QuoteExecArgument(@"/opt/a\b"));
    }

    [Fact]
    public void QuoteAndBacktick_TakeAnEscape()
    {
        Assert.Equal("\"/opt/a\\\\\"b\"", DesktopEntryModel.QuoteExecArgument("/opt/a\"b"));
        Assert.Equal("\"/opt/a\\\\`b\"", DesktopEntryModel.QuoteExecArgument("/opt/a`b"));
    }

    [Fact]
    public void PercentSign_IsDoubledSoItIsNotAFieldCode()
    {
        Assert.Equal("\"/opt/50%%.AppImage\"", DesktopEntryModel.QuoteExecArgument("/opt/50%.AppImage"));
    }

    [Fact]
    public void Newline_TabAndReturn_TakeTheirEscapeSequences()
    {
        Assert.Equal("\"/opt/a\\nb\\tc\\rd\"", DesktopEntryModel.QuoteExecArgument("/opt/a\nb\tc\rd"));
    }

    [Fact]
    public void EscapedPathIsWrittenIntoTheExecLine()
    {
        var entry = DesktopEntryModel.Build("bae", "/home/a b/bae$1.AppImage");

        Assert.Contains("Exec=\"/home/a b/bae\\\\$1.AppImage\" %u\n", entry);
    }
}
