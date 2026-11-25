//! Two devices syncing through a local backend. `cmdv-docs/scripts/sync-e2e.sh` starts the
//! backend, signs up an Admin user and runs this with:
//! - `CMDV_API_URL`: the backend, such as https://localhost:5000
//! - `CMDV_E2E_TOKEN`: that user's access token
//! - `CMDV_E2E_SYNC_DIR`: the backend's sync data directory
//!
//! Each scenario starts with the user's server file deleted and with new devices.

use std::path::PathBuf;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use cmdv_lib::crypto::hash::keyed_hash;
use cmdv_lib::crypto::keys::MasterKey;
use cmdv_lib::crypto::sync_keys::KeyBundle;
use cmdv_lib::db::{ClipboardEntry, Database, EntryType, NewEntry};
use cmdv_lib::sync::client::{AccessTokenSource, SyncClient, TokenFuture};
use cmdv_lib::sync::engine::{SyncEngine, SyncError, SyncMode, SyncReport};
use cmdv_lib::sync::item::{self, EntryPayload};

/// Eleven make more than the server's 50 MiB; ten pinned fit under it.
const BIG: usize = 4_900_000;

struct Env {
    api: String,
    token: String,
    sync_file: PathBuf,
}

impl Env {
    fn from_vars() -> Self {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
        let token = var("CMDV_E2E_TOKEN");
        let payload = token.split('.').nth(1).expect("token is a JWT");
        let claims: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
        let user_id = claims["sub"].as_str().expect("token has a sub claim");
        Self {
            api: var("CMDV_API_URL"),
            sync_file: PathBuf::from(var("CMDV_E2E_SYNC_DIR")).join(format!("{user_id}.db")),
            token,
        }
    }

    fn client(&self) -> SyncClient {
        SyncClient::new(&self.api, Arc::new(FixedToken(self.token.clone())))
    }

    /// Deletes the user's server file, as if the server lost it. No request is running, since
    /// the scenarios run one at a time and the server opens the file only per request.
    fn reset_server(&self) {
        for path in [
            self.sync_file.clone(),
            self.sync_file.with_extension("db-journal"),
        ] {
            if path.exists() {
                std::fs::remove_file(path).unwrap();
            }
        }
    }

    /// Every item on the server, decrypted with `master`'s keys.
    async fn server_items(&self, master: &MasterKey) -> Vec<(String, Option<EntryPayload>)> {
        let client = self.client();
        let key = client.get_key().await.unwrap();
        let keys = KeyBundle::open(master, &key.bundle.unwrap())
            .unwrap()
            .item_keys();
        let mut items = Vec::new();
        let mut after = 0;
        loop {
            let page = client.get_changes(after, 500).await.unwrap();
            for change in &page.items {
                items.push((
                    change.item.id.clone(),
                    item::decrypt(&keys, &change.item).unwrap(),
                ));
            }
            match page.items.last() {
                Some(last) if page.has_more => after = last.seq,
                _ => return items,
            }
        }
    }
}

struct FixedToken(String);

impl AccessTokenSource for FixedToken {
    fn access_token(&self, _refresh: bool) -> TokenFuture<'_> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

/// A device: its own database, the account's recovery phrase, and a sync engine. The hash key
/// starts as the phrase's own and stays when the phrase changes, as in the app.
struct Device {
    db: Arc<Database>,
    master: MasterKey,
    hash_key: [u8; 32],
    engine: SyncEngine,
}

impl Device {
    fn new(env: &Env, phrase: u8) -> Self {
        let db = Arc::new(Database::open_in_memory().unwrap());
        let master = MasterKey::from_bytes([phrase; 32]);
        Self {
            engine: SyncEngine::new(db.clone(), env.client()),
            hash_key: master.derive_hash_key(),
            master,
            db,
        }
    }

    /// Changes the account's phrase and moves this device to it, as Settings does.
    async fn change_phrase(&mut self, phrase: u8) {
        let new_master = MasterKey::from_bytes([phrase; 32]);
        let rotation = self
            .engine
            .rotate(&new_master, self.hash_key)
            .await
            .unwrap();
        self.engine
            .finish_rotation(&rotation, &self.hash_key)
            .unwrap();
        self.master = new_master;
    }

    /// Changes the phrase on the server but stops before this device moves to it, as if the
    /// app quit right after the server committed.
    async fn change_phrase_and_stop(&self, phrase: u8) {
        let new_master = MasterKey::from_bytes([phrase; 32]);
        self.engine
            .rotate(&new_master, self.hash_key)
            .await
            .unwrap();
    }

    /// Enters a phrase changed on another device, as the Settings form does.
    async fn enter_phrase(&mut self, phrase: u8) {
        let new_master = MasterKey::from_bytes([phrase; 32]);
        let check = self.engine.open_with_phrase(&new_master).await.unwrap();
        self.engine.adopt_phrase(&check, &self.hash_key).unwrap();
        self.hash_key = *check.hash_key();
        self.master = new_master;
    }

    /// Copies like the clipboard monitor does: a re-copy touches the entry, new content is added.
    fn copy(&self, content: impl AsRef<[u8]>) {
        self.add(content.as_ref(), false);
    }

    fn copy_local_only(&self, content: &str) {
        self.add(content.as_bytes(), true);
    }

    fn add(&self, content: &[u8], local_only: bool) {
        let hash = keyed_hash(&self.hash_key, content);
        if !self.db.touch_entry_by_hash(&hash).unwrap() {
            self.db
                .insert_entry(&NewEntry {
                    content: content.to_vec(),
                    content_type: EntryType::Text,
                    content_hash: hash,
                    size_bytes: content.len() as i64,
                    is_favorite: false,
                    local_only,
                })
                .unwrap();
        }
    }

    fn entry(&self, content: impl AsRef<[u8]>) -> Option<ClipboardEntry> {
        self.db
            .get_all_entries()
            .unwrap()
            .into_iter()
            .find(|e| e.content == content.as_ref())
    }

    fn has(&self, content: impl AsRef<[u8]>) -> bool {
        self.entry(content).is_some()
    }

    fn toggle_favorite(&self, content: impl AsRef<[u8]>) {
        self.db
            .toggle_favorite(&self.entry(content).unwrap().id)
            .unwrap();
    }

    fn delete(&self, content: impl AsRef<[u8]>) {
        self.db
            .delete_entry(&self.entry(content).unwrap().id)
            .unwrap();
    }

    async fn try_sync(&self) -> Result<SyncReport, SyncError> {
        self.engine.run(&self.master, SyncMode::Full).await
    }

    async fn sync(&self) -> SyncReport {
        self.try_sync().await.unwrap()
    }
}

fn big(n: u8) -> Vec<u8> {
    vec![n; BIG]
}

#[tokio::test]
#[ignore = "needs a local backend; run cmdv-docs/scripts/sync-e2e.sh"]
async fn two_devices_sync() {
    let env = Env::from_vars();

    env.reset_server();
    uploads_arrive_on_the_other_device(&env).await;
    println!("scenario 1 ok: entries upload and arrive on the other device");

    env.reset_server();
    a_delete_reaches_the_other_device_after_many_changes(&env).await;
    println!("scenario 2 ok: a delete reaches the other device after many later changes");

    env.reset_server();
    offline_changes_upload_in_order(&env).await;
    println!("scenario 3 ok: changes made offline upload in order");

    env.reset_server();
    the_same_text_on_two_devices_is_one_item(&env).await;
    println!("scenario 4 ok: the same text on two devices is one item");

    env.reset_server();
    the_server_trims_and_refuses_pins_over_the_limit(&env).await;
    println!("scenario 5 ok: the server trims to 50 MiB and refuses pins past it");

    env.reset_server();
    deletes_survive_a_server_restore(&env).await;
    println!("scenario 6 ok: a delete survives a server restore, and a lost server file");

    env.reset_server();
    another_phrase_is_refused(&env).await;
    println!("scenario 7 ok: a device with another recovery phrase is refused");

    // Scenario 8 runs inside scenario 6, through the restore.
    println!("scenario 8 ok: a local-only entry never reaches the server and stays on its device");

    env.reset_server();
    a_phrase_change_reaches_the_other_device(&env).await;
    println!(
        "scenario 9 ok: a phrase change pauses the other device until it enters the new phrase"
    );
}

async fn a_phrase_change_reaches_the_other_device(env: &Env) {
    let mut a = Device::new(env, 1);
    let mut b = Device::new(env, 1);

    a.copy("before");
    a.copy("deleted before the change");
    a.sync().await;
    b.sync().await;
    a.delete("deleted before the change");
    a.sync().await;
    b.copy("copied on B meanwhile");

    a.change_phrase(3).await;
    assert!(matches!(
        b.try_sync().await,
        Err(SyncError::KeyVersionBehind)
    ));
    assert!(matches!(
        b.engine
            .open_with_phrase(&MasterKey::from_bytes([9; 32]))
            .await,
        Err(SyncError::WrongPhrase)
    ));

    b.enter_phrase(3).await;
    assert!(b.sync().await.restored);
    a.sync().await;
    assert!(a.has("copied on B meanwhile"));
    assert!(b.has("before"));
    assert!(!b.has("deleted before the change"));

    // Every item on the server is under the new key version: server_items decrypts each one
    // with the new phrase's keys and fails on any other.
    assert_eq!(env.client().get_key().await.unwrap().key_version, 2);
    let items = env.server_items(&a.master).await;
    assert!(items.iter().any(|(_, e)| e
        .as_ref()
        .is_some_and(|e| e.content == b"copied on B meanwhile")));

    // If the app stops right after the server commits, the device is simply behind and takes
    // the phrase the user just saved, like any other device.
    a.change_phrase_and_stop(4).await;
    assert!(matches!(
        a.try_sync().await,
        Err(SyncError::KeyVersionBehind)
    ));
    a.enter_phrase(4).await;
    a.sync().await;
    b.enter_phrase(4).await;
    b.sync().await;
    assert!(a.has("before") && a.has("copied on B meanwhile"));
    assert!(b.has("before") && b.has("copied on B meanwhile"));
    assert_eq!(env.client().get_key().await.unwrap().key_version, 3);
    env.server_items(&b.master).await;
}

async fn uploads_arrive_on_the_other_device(env: &Env) {
    let a = Device::new(env, 1);
    let b = Device::new(env, 1);

    a.copy("one");
    a.copy("two");
    a.toggle_favorite("two");
    assert_eq!(a.sync().await.uploaded, 2);

    b.sync().await;
    assert!(b.has("one"));
    assert!(b.entry("two").unwrap().is_favorite);
}

async fn a_delete_reaches_the_other_device_after_many_changes(env: &Env) {
    let a = Device::new(env, 1);
    let b = Device::new(env, 1);

    a.copy("keep");
    a.copy("gone");
    a.sync().await;
    b.sync().await;
    assert!(b.has("gone"));

    a.delete("gone");
    for i in 0..50 {
        a.copy(format!("later {i}"));
    }
    a.sync().await;

    b.sync().await;
    assert!(!b.has("gone"));
    assert!(b.has("keep"));
    assert!(b.has("later 49"));
}

async fn offline_changes_upload_in_order(env: &Env) {
    let a = Device::new(env, 1);
    let b = Device::new(env, 1);

    a.copy("x");
    a.copy("y");
    a.toggle_favorite("x");
    a.delete("y");
    a.copy("z");
    a.copy("y");
    a.delete("z");
    a.sync().await;

    b.sync().await;
    assert!(b.entry("x").unwrap().is_favorite);
    assert!(b.has("y"));
    assert!(!b.has("z"));
}

async fn the_same_text_on_two_devices_is_one_item(env: &Env) {
    let a = Device::new(env, 1);
    let b = Device::new(env, 1);

    a.copy("same");
    b.copy("same");
    a.sync().await;
    b.sync().await;
    a.sync().await;

    let items = env.server_items(&a.master).await;
    let same: Vec<_> = items
        .iter()
        .filter(|(_, entry)| entry.as_ref().is_some_and(|e| e.content == b"same"))
        .collect();
    assert_eq!(same.len(), 1);
    assert_eq!(a.db.get_entry_count().unwrap(), 1);
    assert_eq!(b.db.get_entry_count().unwrap(), 1);
}

async fn the_server_trims_and_refuses_pins_over_the_limit(env: &Env) {
    let a = Device::new(env, 1);
    let b = Device::new(env, 1);

    a.copy("small");
    a.sync().await;
    b.sync().await;

    for n in 0..11 {
        a.copy(big(n));
    }
    a.sync().await;
    b.sync().await;

    // The oldest entries went from the server, and stay on the devices that had them.
    let on_server: Vec<Vec<u8>> = env
        .server_items(&a.master)
        .await
        .into_iter()
        .filter_map(|(_, entry)| entry.map(|e| e.content))
        .collect();
    assert!(!on_server.contains(&b"small".to_vec()));
    assert!(!on_server.contains(&big(0)));
    assert!(on_server.contains(&big(10)));
    assert!(a.has("small") && a.has(big(0)));
    assert!(b.has("small"));

    for n in 0..11 {
        a.toggle_favorite(big(n));
    }
    let report = a.sync().await;
    assert!(report.pinned_limit);
    assert!(!a.entry(big(10)).unwrap().is_favorite);
    assert!((0..10).all(|n| a.entry(big(n)).unwrap().is_favorite));
}

async fn deletes_survive_a_server_restore(env: &Env) {
    let a = Device::new(env, 1);
    let b = Device::new(env, 1);

    a.copy("stays");
    a.copy("deleted after the backup");
    a.copy_local_only("local secret");
    a.sync().await;
    b.sync().await;
    assert!(b.has("deleted after the backup"));
    assert!(!b.has("local secret"));

    let backup = std::fs::read(&env.sync_file).unwrap();
    a.delete("deleted after the backup");
    a.sync().await;
    std::fs::write(&env.sync_file, &backup).unwrap();

    assert!(!b.sync().await.restored);
    assert!(a.sync().await.restored);
    b.sync().await;
    assert!(!a.has("deleted after the backup"));
    assert!(!b.has("deleted after the backup"));
    assert!(b.has("stays"));

    // Scenario 8: the local-only entry stayed on A and never reached the server.
    assert!(a.entry("local secret").unwrap().local_only);
    let items = env.server_items(&a.master).await;
    assert!(items.iter().all(|(_, entry)| entry
        .as_ref()
        .map_or(true, |e| e.content != b"local secret")));

    // A server that lost the file entirely gets the devices' key again and starts over.
    env.reset_server();
    assert!(a.sync().await.restored);
    assert!(b.sync().await.restored);
    b.copy("after the loss");
    b.sync().await;
    a.sync().await;
    assert!(a.has("after the loss"));
}

async fn another_phrase_is_refused(env: &Env) {
    let a = Device::new(env, 1);
    a.copy("x");
    a.sync().await;

    let other = Device::new(env, 2);
    assert!(matches!(
        other.try_sync().await,
        Err(SyncError::DifferentPhrase)
    ));
}
