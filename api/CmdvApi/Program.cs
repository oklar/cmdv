using System.Security.Claims;
using System.Threading.RateLimiting;
using Amazon.S3;
using CmdvApi.Data;
using CmdvApi.Models;
using CmdvApi.Services;
using Microsoft.AspNetCore.Authentication.JwtBearer;
using Microsoft.EntityFrameworkCore;
using Microsoft.IdentityModel.Tokens;

var builder = WebApplication.CreateBuilder(args);

builder.Services.AddDbContext<AppDbContext>(options =>
    options.UseSqlite(builder.Configuration.GetConnectionString("DefaultConnection")
                      ?? "Data Source=cmdv.db"));

builder.Services.AddAuthentication(JwtBearerDefaults.AuthenticationScheme)
    .AddJwtBearer(options =>
    {
        var key = Convert.FromBase64String(builder.Configuration["Jwt:Key"]
            ?? throw new InvalidOperationException("Jwt:Key not configured"));
        options.TokenValidationParameters = new TokenValidationParameters
        {
            ValidateIssuer = true,
            ValidateAudience = true,
            ValidateLifetime = true,
            ValidateIssuerSigningKey = true,
            ValidIssuer = builder.Configuration["Jwt:Issuer"],
            ValidAudience = builder.Configuration["Jwt:Audience"],
            IssuerSigningKey = new SymmetricSecurityKey(key),
            ClockSkew = TimeSpan.FromSeconds(30),
        };
    });

builder.Services.AddAuthorization();

builder.Services.AddSingleton<IAmazonS3>(sp =>
{
    var config = sp.GetRequiredService<IConfiguration>();
    var s3Config = new AmazonS3Config
    {
        ServiceURL = config["R2:Endpoint"] ?? "https://r2.cloudflarestorage.com",
        ForcePathStyle = true,
    };
    return new AmazonS3Client(
        config["R2:AccessKeyId"] ?? "",
        config["R2:SecretAccessKey"] ?? "",
        s3Config);
});

builder.Services.AddScoped<AuthService>();
builder.Services.AddScoped<TierService>();
builder.Services.AddScoped<R2Service>();
builder.Services.AddScoped<PaddleService>();
builder.Services.AddControllers();

builder.Services.AddRateLimiter(options =>
{
    options.GlobalLimiter = PartitionedRateLimiter.Create<HttpContext, string>(context =>
        RateLimitPartition.GetFixedWindowLimiter(
            partitionKey: context.Connection.RemoteIpAddress?.ToString() ?? "unknown",
            factory: _ => new FixedWindowRateLimiterOptions
            {
                PermitLimit = 100,
                Window = TimeSpan.FromMinutes(1),
            }));
    options.RejectionStatusCode = 429;
});

var app = builder.Build();

using (var scope = app.Services.CreateScope())
{
    var db = scope.ServiceProvider.GetRequiredService<AppDbContext>();
    db.Database.EnsureCreated();
}

app.UseRateLimiter();
app.UseAuthentication();
app.UseAuthorization();
app.MapControllers();

int GetUserId(ClaimsPrincipal user) =>
    int.Parse(user.FindFirstValue(ClaimTypes.NameIdentifier)!);

app.MapPost("/auth/register", async (RegisterRequest req, AuthService auth) =>
{
    var result = await auth.Register(req);
    return result is null ? Results.Conflict("Email already registered") : Results.Ok(result);
});

app.MapPost("/auth/login", async (LoginRequest req, AuthService auth) =>
{
    var result = await auth.Login(req);
    return result is null ? Results.Unauthorized() : Results.Ok(result);
});

app.MapPost("/auth/refresh", async (RefreshRequest req, AuthService auth) =>
{
    var result = await auth.Refresh(req.RefreshToken);
    return result is null ? Results.Unauthorized() : Results.Ok(result);
});

app.MapDelete("/auth/delete", async (HttpContext ctx, AuthService auth, R2Service r2) =>
{
    var userId = GetUserId(ctx.User);
    await r2.DeleteBlob(userId);
    var deleted = await auth.DeleteAccount(userId);
    return deleted ? Results.Ok() : Results.NotFound();
}).RequireAuthorization();

app.MapGet("/sync/blob", async (HttpContext ctx, TierService tiers, R2Service r2) =>
{
    var userId = GetUserId(ctx.User);
    if (!await tiers.CanSync(userId))
        return Results.Forbid();

    await tiers.RecordSync(userId);
    var url = r2.GenerateDownloadUrl(userId);
    return Results.Ok(new SignedUrlResponse { Url = url });
}).RequireAuthorization();

app.MapGet("/sync/blob/upload", async (HttpContext ctx, TierService tiers, R2Service r2) =>
{
    var userId = GetUserId(ctx.User);
    if (!await tiers.CanSync(userId))
        return Results.Forbid();

    await tiers.RecordSync(userId);
    var url = r2.GenerateUploadUrl(userId);
    return Results.Ok(new SignedUrlResponse { Url = url });
}).RequireAuthorization();

app.MapGet("/sync/status", async (HttpContext ctx, TierService tiers) =>
{
    var userId = GetUserId(ctx.User);
    var status = await tiers.GetSyncStatus(userId);
    return Results.Ok(status);
}).RequireAuthorization();

app.MapGet("/subscription/status", async (HttpContext ctx, AppDbContext db) =>
{
    var userId = GetUserId(ctx.User);
    var user = await db.Users.FindAsync(userId);
    if (user is null) return Results.NotFound();

    return Results.Ok(new
    {
        user.Tier,
        user.PaddleSubscriptionId,
        user.SubscriptionExpiresAt,
    });
}).RequireAuthorization();

app.Run();

public partial class Program { }
