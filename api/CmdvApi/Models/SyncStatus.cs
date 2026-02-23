namespace CmdvApi.Models;

public class SyncStatusResponse
{
    public int SyncsRemainingToday { get; set; }
    public int RolloverBalance { get; set; }
    public string? LastSyncAt { get; set; }
}

public class SignedUrlResponse
{
    public required string Url { get; set; }
    public string? Etag { get; set; }
}
