use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};

use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop};

const SALT_HASH_KEY: &[u8] = b"cmdv-hash-key";
const SALT_BLOB_ENC: &[u8] = b"cmdv-blob-encryption";
const SALT_DB_ENC: &[u8] = b"cmdv-db-encryption";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn generate() -> Self {
        let mut key = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut key);
        Self(key)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn derive_hash_key(&self) -> [u8; 32] {
        derive_key(&self.0, SALT_HASH_KEY)
    }

    pub fn derive_blob_key(&self) -> [u8; 32] {
        derive_key(&self.0, SALT_BLOB_ENC)
    }

    pub fn derive_db_key(&self) -> [u8; 32] {
        derive_key(&self.0, SALT_DB_ENC)
    }
}

pub struct AppKeys {
    pub hash_key: [u8; 32],
    pub db_key: [u8; 32],
}

impl Drop for AppKeys {
    fn drop(&mut self) {
        mlock::unlock_mem(&self.hash_key);
        mlock::unlock_mem(&self.db_key);
        self.hash_key.zeroize();
        self.db_key.zeroize();
    }
}

impl AppKeys {
    pub fn new(hash_key: [u8; 32], db_key: [u8; 32]) -> Self {
        let keys = Self { hash_key, db_key };
        mlock::lock_mem(&keys.hash_key);
        mlock::lock_mem(&keys.db_key);
        keys
    }
}

mod mlock {
    /// Prevent memory from being swapped to disk.
    pub fn lock_mem(data: &[u8]) {
        if data.is_empty() {
            return;
        }
        unsafe {
            platform_lock(data.as_ptr(), data.len());
        }
    }

    /// Allow memory to be swapped again.
    pub fn unlock_mem(data: &[u8]) {
        if data.is_empty() {
            return;
        }
        unsafe {
            platform_unlock(data.as_ptr(), data.len());
        }
    }

    #[cfg(unix)]
    unsafe fn platform_lock(ptr: *const u8, len: usize) {
        libc::mlock(ptr as *const libc::c_void, len);
    }

    #[cfg(unix)]
    unsafe fn platform_unlock(ptr: *const u8, len: usize) {
        libc::munlock(ptr as *const libc::c_void, len);
    }

    #[cfg(windows)]
    unsafe fn platform_lock(ptr: *const u8, len: usize) {
        windows_sys::Win32::System::Memory::VirtualLock(ptr as *mut core::ffi::c_void, len);
    }

    #[cfg(windows)]
    unsafe fn platform_unlock(ptr: *const u8, len: usize) {
        windows_sys::Win32::System::Memory::VirtualUnlock(ptr as *mut core::ffi::c_void, len);
    }

    #[cfg(not(any(unix, windows)))]
    unsafe fn platform_lock(_ptr: *const u8, _len: usize) {}
    #[cfg(not(any(unix, windows)))]
    unsafe fn platform_unlock(_ptr: *const u8, _len: usize) {}
}

pub struct VaultState {
    pub keys: Mutex<Option<AppKeys>>,
    pub monitor_stop: Arc<AtomicBool>,
    pub monitor_wake: Arc<(Mutex<bool>, Condvar)>,
    pub setup_complete: AtomicBool,
    /// Next clipboard text with this content hash is ignored (e.g. secure-paste link).
    pub clipboard_skip_hash: Arc<Mutex<Option<Vec<u8>>>>,
    /// True while `run_create_secure_paste` is running (blocks overlapping hotkey fires).
    pub secure_paste_in_flight: Arc<AtomicBool>,
}

impl VaultState {
    pub fn new() -> Self {
        Self {
            keys: Mutex::new(None),
            monitor_stop: Arc::new(AtomicBool::new(true)),
            monitor_wake: Arc::new((Mutex::new(false), Condvar::new())),
            setup_complete: AtomicBool::new(false),
            clipboard_skip_hash: Arc::new(Mutex::new(None)),
            secure_paste_in_flight: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn derive_key(ikm: &[u8], info: &[u8]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, ikm);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm)
        .expect("HKDF expand should not fail for 32 bytes");
    okm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_key_derivation_produces_distinct_keys() {
        let mk = MasterKey::generate();
        let hash_key = mk.derive_hash_key();
        let blob_key = mk.derive_blob_key();
        let db_key = mk.derive_db_key();
        assert_ne!(hash_key, blob_key);
        assert_ne!(hash_key, db_key);
        assert_ne!(blob_key, db_key);
    }

    #[test]
    fn master_key_derivation_is_deterministic() {
        let bytes = [99u8; 32];
        let mk1 = MasterKey::from_bytes(bytes);
        let mk2 = MasterKey::from_bytes(bytes);
        assert_eq!(mk1.derive_hash_key(), mk2.derive_hash_key());
    }
}
