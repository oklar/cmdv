using CmdvApi.Data;
using Microsoft.EntityFrameworkCore;
using Microsoft.Extensions.Configuration;

namespace CmdvApi.Tests;

public static class TestHelpers
{
    private static int _dbCounter;

    public static AppDbContext CreateTestDb()
    {
        var dbName = $"TestDb_{Interlocked.Increment(ref _dbCounter)}";
        var options = new DbContextOptionsBuilder<AppDbContext>()
            .UseInMemoryDatabase(dbName)
            .Options;
        return new AppDbContext(options);
    }

    public static IConfiguration CreateTestConfig()
    {
        var key = Convert.ToBase64String(new byte[32]);
        return new ConfigurationBuilder()
            .AddInMemoryCollection(new Dictionary<string, string?>
            {
                ["Jwt:Key"] = key,
                ["Jwt:Issuer"] = "test-issuer",
                ["Jwt:Audience"] = "test-audience",
                ["R2:Endpoint"] = "https://test.r2.cloudflarestorage.com",
                ["R2:AccessKeyId"] = "test-key",
                ["R2:SecretAccessKey"] = "test-secret",
                ["R2:BucketName"] = "test-bucket",
                ["Paddle:WebhookSecret"] = "test-webhook-secret",
            })
            .Build();
    }
}
