pub mod keychain;

pub trait SeedStore: Send + Sync {
    fn save_seed(&self, seed: &[u8]) -> Result<(), String>;
    fn load_seed(&self) -> Result<Vec<u8>, String>;
    fn delete_seed(&self) -> Result<(), String>;
    fn exists(&self) -> Result<bool, String>;
}
