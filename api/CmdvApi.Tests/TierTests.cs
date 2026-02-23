using CmdvApi.Models;
using CmdvApi.Services;

namespace CmdvApi.Tests;

public class TierTests
{
    private (TierService tiers, CmdvApi.Data.AppDbContext db) CreateTierService()
    {
        var db = TestHelpers.CreateTestDb();
        return (new TierService(db), db);
    }

    private User CreatePaidUser(CmdvApi.Data.AppDbContext db)
    {
        var user = new User
        {
            Email = $"paid-{Guid.NewGuid()}@test.com",
            AuthHash = new byte[32],
            EncryptedMasterKey = new byte[64],
            Tier = "paid",
            SyncCountToday = 0,
            RolloverBalance = 0,
            LastSyncResetDate = DateTime.UtcNow.Date,
        };
        db.Users.Add(user);
        db.SaveChanges();
        return user;
    }

    private User CreateFreeUser(CmdvApi.Data.AppDbContext db)
    {
        var user = new User
        {
            Email = $"free-{Guid.NewGuid()}@test.com",
            AuthHash = new byte[32],
            EncryptedMasterKey = new byte[64],
            Tier = "free",
        };
        db.Users.Add(user);
        db.SaveChanges();
        return user;
    }

    [Fact]
    public async Task FreeUser_CannotSync()
    {
        var (tiers, db) = CreateTierService();
        var user = CreateFreeUser(db);
        Assert.False(await tiers.CanSync(user.Id));
    }

    [Fact]
    public async Task PaidUser_CanSync()
    {
        var (tiers, db) = CreateTierService();
        var user = CreatePaidUser(db);
        Assert.True(await tiers.CanSync(user.Id));
    }

    [Fact]
    public async Task RecordSync_IncrementsCount()
    {
        var (tiers, db) = CreateTierService();
        var user = CreatePaidUser(db);

        await tiers.RecordSync(user.Id);
        var status = await tiers.GetSyncStatus(user.Id);

        Assert.Equal(199, status.SyncsRemainingToday);
    }

    [Fact]
    public async Task DailyLimit_Enforced()
    {
        var (tiers, db) = CreateTierService();
        var user = CreatePaidUser(db);
        user.SyncCountToday = 200;
        db.SaveChanges();

        Assert.False(await tiers.CanSync(user.Id));
    }

    [Fact]
    public async Task Rollover_AllowsSyncBeyondDailyLimit()
    {
        var (tiers, db) = CreateTierService();
        var user = CreatePaidUser(db);
        user.SyncCountToday = 200;
        user.RolloverBalance = 50;
        db.SaveChanges();

        Assert.True(await tiers.CanSync(user.Id));
    }

    [Fact]
    public async Task Rollover_CappedAt1000()
    {
        var (tiers, db) = CreateTierService();
        var user = CreatePaidUser(db);
        user.SyncCountToday = 0;
        user.RolloverBalance = 980;
        user.LastSyncResetDate = DateTime.UtcNow.Date.AddDays(-1);
        db.SaveChanges();

        var status = await tiers.GetSyncStatus(user.Id);
        Assert.Equal(1000, status.RolloverBalance);
    }

    [Fact]
    public async Task DailyReset_ClearsCount()
    {
        var (tiers, db) = CreateTierService();
        var user = CreatePaidUser(db);
        user.SyncCountToday = 150;
        user.LastSyncResetDate = DateTime.UtcNow.Date.AddDays(-1);
        db.SaveChanges();

        var status = await tiers.GetSyncStatus(user.Id);
        Assert.Equal(200 + Math.Min(1000, 50), status.SyncsRemainingToday);
    }

    [Fact]
    public async Task NonexistentUser_CannotSync()
    {
        var (tiers, _) = CreateTierService();
        Assert.False(await tiers.CanSync(99999));
    }
}
