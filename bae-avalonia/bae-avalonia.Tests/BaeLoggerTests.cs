using Xunit;

namespace Bae.Desktop.Tests;

// A logged failure keeps everything that explains it, not only its type and
// message.
public sealed class BaeLoggerTests
{
    [Fact]
    public void ALoggedExceptionKeepsItsStackAndCauses()
    {
        Exception caught;
        try
        {
            Fail();
            return;
        }
        catch (InvalidOperationException exception)
        {
            caught = exception;
        }

        var line = BaeLogger.Format("Opening the library failed", caught);

        Assert.Contains("Opening the library failed", line);
        Assert.Contains("the store is locked", line);
        Assert.Contains("disk is full", line);
        Assert.Contains(nameof(Fail), line);
    }

    private static void Fail()
    {
        try
        {
            throw new IOException("disk is full");
        }
        catch (IOException cause)
        {
            throw new InvalidOperationException("the store is locked", cause);
        }
    }
}
