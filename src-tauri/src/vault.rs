use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose, Engine as _};
use parking_lot::Mutex;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

const VAULT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl Entry {
    pub fn new(title: impl Into<String>, content: impl Into<String>) -> Self {
        let now = OffsetDateTime::now_utc();
        Self {
            id: Uuid::new_v4(),
            title: title.into(),
            content: content.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }
}

#[derive(Debug, Serialize)]
pub struct UnlockResponse {
    pub entries: Vec<Entry>,
    pub created: bool,
    pub last_saved: Option<String>,
}

#[derive(Default)]
pub struct VaultManager {
    inner: Mutex<Option<UnlockedVault>>,
}

impl VaultManager {
    pub fn unlock(&self, passphrase: &str, vault_path: PathBuf) -> Result<UnlockResponse> {
        if !vault_path.exists() {
            let mut salt = [0u8; 16];
            OsRng.fill_bytes(&mut salt);
            let key = derive_key(passphrase, &salt)?;
            let entries: Vec<Entry> = Vec::new();
            save_vault(&vault_path, &salt, &key, &entries)?;

            let unlocked = UnlockedVault {
                key,
                salt,
                entries,
                path: vault_path,
                last_saved: OffsetDateTime::now_utc(),
            };

            let response = UnlockResponse {
                entries: Vec::new(),
                created: true,
                last_saved: unlocked.last_saved.format(&Rfc3339).ok(),
            };

            *self.inner.lock() = Some(unlocked);
            return Ok(response);
        }

        let stored = load_vault(&vault_path)?;
        let salt_vec = general_purpose::STANDARD_NO_PAD
            .decode(&stored.salt)
            .context("invalid salt encoding")?;
        if salt_vec.len() != 16 {
            return Err(anyhow!("invalid salt length"));
        }
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&salt_vec);

        let key = derive_key(passphrase, &salt)?;
        let entries = decrypt_entries(&stored, &key)?;

        let last_saved = stored
            .updated_at
            .and_then(|ts| ts.format(&Rfc3339).ok());

        let unlocked = UnlockedVault {
            key,
            salt,
                entries: entries.clone(),
            path: vault_path,
            last_saved: stored.updated_at.unwrap_or_else(OffsetDateTime::now_utc),
        };

        *self.inner.lock() = Some(unlocked);

        Ok(UnlockResponse {
            entries,
            created: false,
            last_saved,
        })
    }

    pub fn lock(&self) {
        *self.inner.lock() = None;
    }

    pub fn list(&self) -> Result<Vec<Entry>> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        let mut entries = vault.entries.clone();
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(entries)
    }

    pub fn create_entry(&self, title: &str, content: &str) -> Result<Entry> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
    let entry = Entry::new(title, content);
        vault.entries.push(entry.clone());
        save_current(vault)?;
        Ok(entry)
    }

    pub fn update_entry(&self, entry: Entry) -> Result<Entry> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        let updated = {
            let existing = vault
                .entries
                .iter_mut()
                .find(|item| item.id == entry.id)
                .ok_or_else(|| anyhow!("entry not found"))?;
            let mut updated = entry.clone();
            updated.touch();
            *existing = updated.clone();
            updated
        };

        save_current(vault)?;
        Ok(updated)
    }

    pub fn delete_entry(&self, id: Uuid) -> Result<()> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        let len_before = vault.entries.len();
        vault.entries.retain(|entry| entry.id != id);
        if vault.entries.len() == len_before {
            return Err(anyhow!("entry not found"));
        }
        save_current(vault)?;
        Ok(())
    }

    pub fn export_plaintext(&self) -> Result<String> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        let mut lines = Vec::new();
        for entry in vault.entries.iter() {
            lines.push(format!(
                "# {title}\nCreated: {created}\nUpdated: {updated}\n\n{content}\n",
                title = entry.title,
                created = entry
                    .created_at
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| String::new()),
                updated = entry
                    .updated_at
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| String::new()),
                content = entry.content
            ));
        }
        Ok(lines.join("\n---\n\n"))
    }
}

fn save_current(vault: &mut UnlockedVault) -> Result<()> {
    vault.last_saved = OffsetDateTime::now_utc();
    save_vault(&vault.path, &vault.salt, &vault.key, &vault.entries)?;
    Ok(())
}

struct UnlockedVault {
    key: [u8; 32],
    salt: [u8; 16],
    entries: Vec<Entry>,
    path: PathBuf,
    last_saved: OffsetDateTime,
}

#[derive(Serialize, Deserialize)]
struct StoredVault {
    version: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
    updated_at: Option<OffsetDateTime>,
}

fn derive_key(passphrase: &str, salt: &[u8; 16]) -> Result<[u8; 32]> {
    let argon = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(19456, 2, 1, Some(32)).context("invalid argon2 parameters")?,
    );
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .context("failed to derive key")?;
    Ok(key)
}

fn save_vault(path: &PathBuf, salt: &[u8; 16], key: &[u8; 32], entries: &[Entry]) -> Result<()> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid key"))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    #[allow(deprecated)]
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = serde_json::to_vec(entries).context("failed to serialize entries")?;
    let ciphertext = cipher
        .encrypt(nonce, payload.as_ref())
        .map_err(|_| anyhow!("encryption failed"))?;

    let stored = StoredVault {
        version: VAULT_VERSION,
        salt: general_purpose::STANDARD_NO_PAD.encode(salt),
        nonce: general_purpose::STANDARD_NO_PAD.encode(nonce_bytes),
        ciphertext: general_purpose::STANDARD_NO_PAD.encode(ciphertext),
        updated_at: Some(OffsetDateTime::now_utc()),
    };

    let serialized = serde_json::to_string_pretty(&stored).context("failed to serialize vault")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("failed to create vault directory")?;
    }
    fs::write(path, serialized).context("failed to write vault")
}

fn load_vault(path: &PathBuf) -> Result<StoredVault> {
    let content = fs::read_to_string(path).context("failed to read vault")?;
    let stored: StoredVault = serde_json::from_str(&content).context("failed to parse vault")?;
    if stored.version != VAULT_VERSION {
        return Err(anyhow!("unsupported vault version"));
    }
    Ok(stored)
}

fn decrypt_entries(stored: &StoredVault, key: &[u8; 32]) -> Result<Vec<Entry>> {
    let nonce_bytes = general_purpose::STANDARD_NO_PAD
        .decode(&stored.nonce)
        .context("invalid nonce encoding")?;
    let ciphertext = general_purpose::STANDARD_NO_PAD
        .decode(&stored.ciphertext)
        .context("invalid ciphertext encoding")?;

    if nonce_bytes.len() != 12 {
        return Err(anyhow!("invalid nonce length"));
    }

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid key"))?;
    #[allow(deprecated)]
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| anyhow!("decryption failed"))?;

    let entries: Vec<Entry> = serde_json::from_slice(&plaintext).context("invalid entry data")?;
    Ok(entries)
}

pub fn vault_file_path(mut base: PathBuf) -> PathBuf {
    base.push("vault.json");
    base
}
