using CmdvApi.Data;
using CmdvApi.Models;
using Microsoft.EntityFrameworkCore;

namespace CmdvApi.Services;

public class TierService
{
    private const int DailySyncLimit = 200;
    private const int MaxRollover = 1000;

    private readonly AppDbContext _db;

    public TierService(AppDbContext db)
    {
        _db = db;
    }

    public async Task<bool> CanSync(int userId)
    {
        var user = await _db.Users.FindAsync(userId);
        if (user is null) return false;
        if (user.Tier != "paid") return false;

        ResetDailyCountIfNeeded(user);
        return GetAvailableSyncs(user) > 0;
    }

    public async Task<bool> RecordSync(int userId)
    {
        var user = await _db.Users.FindAsync(userId);
        if (user is null) return false;
        if (user.Tier != "paid") return false;

        ResetDailyCountIfNeeded(user);

        if (GetAvailableSyncs(user) <= 0)
            return false;

        if (user.RolloverBalance > 0 && user.SyncCountToday >= DailySyncLimit)
            user.RolloverBalance--;
        else
            user.SyncCountToday++;

        await _db.SaveChangesAsync();
        return true;
    }

    public async Task<SyncStatusResponse> GetSyncStatus(int userId)
    {
        var user = await _db.Users.FindAsync(userId);
        if (user is null)
            return new SyncStatusResponse { SyncsRemainingToday = 0 };

        ResetDailyCountIfNeeded(user);
        await _db.SaveChangesAsync();

        return new SyncStatusResponse
        {
            SyncsRemainingToday = GetAvailableSyncs(user),
            RolloverBalance = user.RolloverBalance,
        };
    }

    private static int GetAvailableSyncs(User user)
    {
        var dailyRemaining = Math.Max(0, DailySyncLimit - user.SyncCountToday);
        return dailyRemaining + user.RolloverBalance;
    }

    private static void ResetDailyCountIfNeeded(User user)
    {
        var today = DateTime.UtcNow.Date;
        if (user.LastSyncResetDate >= today) return;

        var unusedToday = Math.Max(0, DailySyncLimit - user.SyncCountToday);
        user.RolloverBalance = Math.Min(MaxRollover, user.RolloverBalance + unusedToday);
        user.SyncCountToday = 0;
        user.LastSyncResetDate = today;
    }
}
