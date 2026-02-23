using System.IdentityModel.Tokens.Jwt;
using System.Security.Claims;
using System.Security.Cryptography;
using CmdvApi.Data;
using CmdvApi.Models;
using Konscious.Security.Cryptography;
using Microsoft.EntityFrameworkCore;
using Microsoft.IdentityModel.Tokens;

namespace CmdvApi.Services;

public class AuthService
{
    private readonly AppDbContext _db;
    private readonly IConfiguration _config;

    public AuthService(AppDbContext db, IConfiguration config)
    {
        _db = db;
        _config = config;
    }

    public async Task<AuthResponse?> Register(RegisterRequest request)
    {
        if (await _db.Users.AnyAsync(u => u.Email == request.Email))
            return null;

        var authHashBytes = Convert.FromBase64String(request.AuthHash);
        var storedHash = HashAuthHash(authHashBytes);

        var user = new User
        {
            Email = request.Email,
            AuthHash = storedHash,
            EncryptedMasterKey = Convert.FromBase64String(request.EncryptedMasterKey),
        };

        _db.Users.Add(user);
        await _db.SaveChangesAsync();

        return GenerateTokens(user);
    }

    public async Task<AuthResponse?> Login(LoginRequest request)
    {
        var user = await _db.Users.FirstOrDefaultAsync(u => u.Email == request.Email);
        if (user is null) return null;

        var authHashBytes = Convert.FromBase64String(request.AuthHash);
        if (!VerifyAuthHash(authHashBytes, user.AuthHash))
            return null;

        var response = GenerateTokens(user);
        response.EncryptedMasterKey = Convert.ToBase64String(user.EncryptedMasterKey);
        return response;
    }

    public async Task<AuthResponse?> Refresh(string refreshToken)
    {
        var user = await _db.Users.FirstOrDefaultAsync(u =>
            u.RefreshToken == refreshToken && u.RefreshTokenExpiry > DateTime.UtcNow);

        if (user is null) return null;

        return GenerateTokens(user);
    }

    public async Task<bool> DeleteAccount(int userId)
    {
        var user = await _db.Users.FindAsync(userId);
        if (user is null) return false;

        _db.Users.Remove(user);
        await _db.SaveChangesAsync();
        return true;
    }

    private AuthResponse GenerateTokens(User user)
    {
        var accessToken = GenerateAccessToken(user);
        var refreshToken = GenerateRefreshToken();

        user.RefreshToken = refreshToken;
        user.RefreshTokenExpiry = DateTime.UtcNow.AddDays(7);
        _db.SaveChanges();

        return new AuthResponse
        {
            AccessToken = accessToken,
            RefreshToken = refreshToken,
        };
    }

    private string GenerateAccessToken(User user)
    {
        var key = Convert.FromBase64String(_config["Jwt:Key"]!);
        var credentials = new SigningCredentials(
            new SymmetricSecurityKey(key),
            SecurityAlgorithms.HmacSha256);

        var claims = new[]
        {
            new Claim(ClaimTypes.NameIdentifier, user.Id.ToString()),
            new Claim(ClaimTypes.Email, user.Email),
            new Claim("tier", user.Tier),
        };

        var token = new JwtSecurityToken(
            issuer: _config["Jwt:Issuer"],
            audience: _config["Jwt:Audience"],
            claims: claims,
            expires: DateTime.UtcNow.AddMinutes(15),
            signingCredentials: credentials);

        return new JwtSecurityTokenHandler().WriteToken(token);
    }

    private static string GenerateRefreshToken()
    {
        var bytes = new byte[64];
        RandomNumberGenerator.Fill(bytes);
        return Convert.ToBase64String(bytes);
    }

    private static byte[] HashAuthHash(byte[] authHash)
    {
        using var argon2 = new Argon2id(authHash);
        argon2.Salt = "cmdv-server-auth"u8.ToArray();
        argon2.DegreeOfParallelism = 4;
        argon2.MemorySize = 65536;
        argon2.Iterations = 3;
        return argon2.GetBytes(32);
    }

    private static bool VerifyAuthHash(byte[] authHash, byte[] storedHash)
    {
        var computed = HashAuthHash(authHash);
        return CryptographicOperations.FixedTimeEquals(computed, storedHash);
    }
}
