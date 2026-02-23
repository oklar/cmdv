namespace CmdvApi.Models;

public class RegisterRequest
{
    public required string Email { get; set; }
    public required string AuthHash { get; set; }
    public required string EncryptedMasterKey { get; set; }
}

public class LoginRequest
{
    public required string Email { get; set; }
    public required string AuthHash { get; set; }
}

public class RefreshRequest
{
    public required string RefreshToken { get; set; }
}

public class AuthResponse
{
    public required string AccessToken { get; set; }
    public required string RefreshToken { get; set; }
    public string? EncryptedMasterKey { get; set; }
}
