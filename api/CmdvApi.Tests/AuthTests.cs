using CmdvApi.Models;
using CmdvApi.Services;

namespace CmdvApi.Tests;

public class AuthTests
{
    private AuthService CreateAuthService()
    {
        var db = TestHelpers.CreateTestDb();
        var config = TestHelpers.CreateTestConfig();
        return new AuthService(db, config);
    }

    [Fact]
    public async Task Register_ReturnsTokens()
    {
        var auth = CreateAuthService();
        var result = await auth.Register(new RegisterRequest
        {
            Email = "test@example.com",
            AuthHash = Convert.ToBase64String(new byte[32]),
            EncryptedMasterKey = Convert.ToBase64String(new byte[64]),
        });

        Assert.NotNull(result);
        Assert.NotEmpty(result.AccessToken);
        Assert.NotEmpty(result.RefreshToken);
    }

    [Fact]
    public async Task Register_DuplicateEmail_ReturnsNull()
    {
        var auth = CreateAuthService();
        var req = new RegisterRequest
        {
            Email = "dup@example.com",
            AuthHash = Convert.ToBase64String(new byte[32]),
            EncryptedMasterKey = Convert.ToBase64String(new byte[64]),
        };

        await auth.Register(req);
        var result = await auth.Register(req);

        Assert.Null(result);
    }

    [Fact]
    public async Task Login_WithCorrectHash_Succeeds()
    {
        var auth = CreateAuthService();
        var hash = Convert.ToBase64String(new byte[32]);

        await auth.Register(new RegisterRequest
        {
            Email = "login@example.com",
            AuthHash = hash,
            EncryptedMasterKey = Convert.ToBase64String(new byte[64]),
        });

        var result = await auth.Login(new LoginRequest
        {
            Email = "login@example.com",
            AuthHash = hash,
        });

        Assert.NotNull(result);
        Assert.NotNull(result.EncryptedMasterKey);
    }

    [Fact]
    public async Task Login_WithWrongHash_ReturnsNull()
    {
        var auth = CreateAuthService();
        await auth.Register(new RegisterRequest
        {
            Email = "wrong@example.com",
            AuthHash = Convert.ToBase64String(new byte[32]),
            EncryptedMasterKey = Convert.ToBase64String(new byte[64]),
        });

        var result = await auth.Login(new LoginRequest
        {
            Email = "wrong@example.com",
            AuthHash = Convert.ToBase64String(new byte[] { 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32 }),
        });

        Assert.Null(result);
    }

    [Fact]
    public async Task Login_NonexistentUser_ReturnsNull()
    {
        var auth = CreateAuthService();
        var result = await auth.Login(new LoginRequest
        {
            Email = "noone@example.com",
            AuthHash = Convert.ToBase64String(new byte[32]),
        });

        Assert.Null(result);
    }

    [Fact]
    public async Task Refresh_ValidToken_ReturnsNewTokens()
    {
        var auth = CreateAuthService();
        var reg = await auth.Register(new RegisterRequest
        {
            Email = "refresh@example.com",
            AuthHash = Convert.ToBase64String(new byte[32]),
            EncryptedMasterKey = Convert.ToBase64String(new byte[64]),
        });

        Assert.NotNull(reg);
        var refreshed = await auth.Refresh(reg.RefreshToken);

        Assert.NotNull(refreshed);
        Assert.NotEmpty(refreshed.AccessToken);
        Assert.NotEqual(reg.RefreshToken, refreshed.RefreshToken);
    }

    [Fact]
    public async Task Refresh_InvalidToken_ReturnsNull()
    {
        var auth = CreateAuthService();
        var result = await auth.Refresh("invalid-token");
        Assert.Null(result);
    }

    [Fact]
    public async Task DeleteAccount_ExistingUser_ReturnsTrue()
    {
        var auth = CreateAuthService();
        await auth.Register(new RegisterRequest
        {
            Email = "delete@example.com",
            AuthHash = Convert.ToBase64String(new byte[32]),
            EncryptedMasterKey = Convert.ToBase64String(new byte[64]),
        });

        var deleted = await auth.DeleteAccount(1);
        Assert.True(deleted);
    }

    [Fact]
    public async Task DeleteAccount_NonexistentUser_ReturnsFalse()
    {
        var auth = CreateAuthService();
        var deleted = await auth.DeleteAccount(999);
        Assert.False(deleted);
    }
}
