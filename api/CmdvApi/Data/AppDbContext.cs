using CmdvApi.Models;
using Microsoft.EntityFrameworkCore;

namespace CmdvApi.Data;

public class AppDbContext : DbContext
{
    public AppDbContext(DbContextOptions<AppDbContext> options) : base(options) { }

    public DbSet<User> Users => Set<User>();

    protected override void OnModelCreating(ModelBuilder modelBuilder)
    {
        modelBuilder.Entity<User>(entity =>
        {
            entity.HasIndex(u => u.Email).IsUnique();
            entity.Property(u => u.Tier).HasDefaultValue("free");
            entity.Property(u => u.SyncCountToday).HasDefaultValue(0);
            entity.Property(u => u.RolloverBalance).HasDefaultValue(0);
        });
    }
}
