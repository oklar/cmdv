namespace CmdvApi.Models;

public class User
{
    public int Id { get; set; }
    public required string Email { get; set; }
    public required byte[] AuthHash { get; set; }
    public required byte[] EncryptedMasterKey { get; set; }
    public bool EmailVerified { get; set; }
    public string Tier { get; set; } = "free";
    public int SyncCountToday { get; set; }
    public int RolloverBalance { get; set; }
    public DateTime LastSyncResetDate { get; set; } = DateTime.UtcNow.Date;
    public string? PaddleSubscriptionId { get; set; }
    public string? PaddleCustomerId { get; set; }
    public DateTime? SubscriptionExpiresAt { get; set; }
    public string? RefreshToken { get; set; }
    public DateTime? RefreshTokenExpiry { get; set; }
    public DateTime CreatedAt { get; set; } = DateTime.UtcNow;
}
