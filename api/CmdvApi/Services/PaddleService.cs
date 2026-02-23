using System.Security.Cryptography;
using System.Text;
using CmdvApi.Data;
using Microsoft.EntityFrameworkCore;

namespace CmdvApi.Services;

public class PaddleService
{
    private readonly AppDbContext _db;
    private readonly string _webhookSecret;

    public PaddleService(AppDbContext db, IConfiguration config)
    {
        _db = db;
        _webhookSecret = config["Paddle:WebhookSecret"] ?? "";
    }

    public bool VerifySignature(string payload, string signature)
    {
        if (string.IsNullOrEmpty(_webhookSecret)) return false;

        var parts = signature.Split(';');
        if (parts.Length < 2) return false;

        var ts = parts[0].Replace("ts=", "");
        var h1 = parts[1].Replace("h1=", "");

        var signedPayload = $"{ts}.{payload}";
        using var hmac = new HMACSHA256(Encoding.UTF8.GetBytes(_webhookSecret));
        var computed = Convert.ToHexStringLower(hmac.ComputeHash(Encoding.UTF8.GetBytes(signedPayload)));

        return CryptographicOperations.FixedTimeEquals(
            Encoding.UTF8.GetBytes(computed),
            Encoding.UTF8.GetBytes(h1));
    }

    public async Task HandleSubscriptionCreated(string customerId, string subscriptionId, DateTime expiresAt)
    {
        var user = await _db.Users.FirstOrDefaultAsync(u => u.PaddleCustomerId == customerId);
        if (user is null) return;

        user.Tier = "paid";
        user.PaddleSubscriptionId = subscriptionId;
        user.SubscriptionExpiresAt = expiresAt;
        await _db.SaveChangesAsync();
    }

    public async Task HandleSubscriptionCancelled(string subscriptionId)
    {
        var user = await _db.Users.FirstOrDefaultAsync(u => u.PaddleSubscriptionId == subscriptionId);
        if (user is null) return;

        user.Tier = "free";
        user.PaddleSubscriptionId = null;
        user.SubscriptionExpiresAt = null;
        await _db.SaveChangesAsync();
    }

    public async Task HandleSubscriptionUpdated(string subscriptionId, DateTime newExpiresAt)
    {
        var user = await _db.Users.FirstOrDefaultAsync(u => u.PaddleSubscriptionId == subscriptionId);
        if (user is null) return;

        user.SubscriptionExpiresAt = newExpiresAt;
        await _db.SaveChangesAsync();
    }
}
