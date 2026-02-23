using System.Security.Cryptography;
using System.Text;
using CmdvApi.Models;
using CmdvApi.Services;

namespace CmdvApi.Tests;

public class PaddleTests
{
    private (PaddleService paddle, CmdvApi.Data.AppDbContext db) CreatePaddleService()
    {
        var db = TestHelpers.CreateTestDb();
        var config = TestHelpers.CreateTestConfig();
        return (new PaddleService(db, config), db);
    }

    [Fact]
    public void VerifySignature_ValidSignature_ReturnsTrue()
    {
        var (paddle, _) = CreatePaddleService();
        var payload = "{\"event_type\":\"test\"}";
        var ts = DateTimeOffset.UtcNow.ToUnixTimeSeconds().ToString();
        var signedPayload = $"{ts}.{payload}";

        using var hmac = new HMACSHA256(Encoding.UTF8.GetBytes("test-webhook-secret"));
        var hash = Convert.ToHexStringLower(hmac.ComputeHash(Encoding.UTF8.GetBytes(signedPayload)));
        var signature = $"ts={ts};h1={hash}";

        Assert.True(paddle.VerifySignature(payload, signature));
    }

    [Fact]
    public void VerifySignature_InvalidSignature_ReturnsFalse()
    {
        var (paddle, _) = CreatePaddleService();
        Assert.False(paddle.VerifySignature("payload", "ts=123;h1=invalidhash"));
    }

    [Fact]
    public void VerifySignature_MalformedSignature_ReturnsFalse()
    {
        var (paddle, _) = CreatePaddleService();
        Assert.False(paddle.VerifySignature("payload", "garbage"));
    }

    [Fact]
    public async Task HandleSubscriptionCreated_UpgradesTier()
    {
        var (paddle, db) = CreatePaddleService();
        var user = new User
        {
            Email = "sub@test.com",
            AuthHash = new byte[32],
            EncryptedMasterKey = new byte[64],
            PaddleCustomerId = "ctm_123",
            Tier = "free",
        };
        db.Users.Add(user);
        db.SaveChanges();

        await paddle.HandleSubscriptionCreated("ctm_123", "sub_456", DateTime.UtcNow.AddYears(1));

        db.Entry(user).Reload();
        Assert.Equal("paid", user.Tier);
        Assert.Equal("sub_456", user.PaddleSubscriptionId);
    }

    [Fact]
    public async Task HandleSubscriptionCancelled_DowngradesToFree()
    {
        var (paddle, db) = CreatePaddleService();
        var user = new User
        {
            Email = "cancel@test.com",
            AuthHash = new byte[32],
            EncryptedMasterKey = new byte[64],
            PaddleSubscriptionId = "sub_789",
            Tier = "paid",
        };
        db.Users.Add(user);
        db.SaveChanges();

        await paddle.HandleSubscriptionCancelled("sub_789");

        db.Entry(user).Reload();
        Assert.Equal("free", user.Tier);
        Assert.Null(user.PaddleSubscriptionId);
    }

    [Fact]
    public async Task HandleSubscriptionUpdated_UpdatesExpiry()
    {
        var (paddle, db) = CreatePaddleService();
        var newExpiry = DateTime.UtcNow.AddYears(2);
        var user = new User
        {
            Email = "update@test.com",
            AuthHash = new byte[32],
            EncryptedMasterKey = new byte[64],
            PaddleSubscriptionId = "sub_update",
            Tier = "paid",
        };
        db.Users.Add(user);
        db.SaveChanges();

        await paddle.HandleSubscriptionUpdated("sub_update", newExpiry);

        db.Entry(user).Reload();
        Assert.NotNull(user.SubscriptionExpiresAt);
        Assert.Equal(newExpiry, user.SubscriptionExpiresAt.Value, TimeSpan.FromSeconds(1));
    }

    [Fact]
    public async Task HandleSubscriptionCreated_UnknownCustomer_DoesNotThrow()
    {
        var (paddle, _) = CreatePaddleService();
        await paddle.HandleSubscriptionCreated("unknown_ctm", "sub_x", DateTime.UtcNow);
    }
}
