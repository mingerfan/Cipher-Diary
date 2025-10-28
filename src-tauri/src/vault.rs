use aes::Aes256;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, Context, Result};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose, Engine as _};
use ctr::cipher::{KeyIvInit, StreamCipher};
use parking_lot::Mutex;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

type Aes256Ctr = ctr::Ctr128BE<Aes256>;

const VAULT_VERSION: u32 = 1;
const METADATA_VERSION: u32 = 1;
const ENTRY_VERSION: u32 = 1;
const IMAGE_MAGIC_PREFIX: &[u8] = b"VAULTIMG"; // 加密图片的固定前缀

const SUPPORTED_TEXT_ENCRYPTIONS: [TextEncryption; 1] = [TextEncryption::Aes256Gcm];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextEncryption {
    Aes256Gcm,
}

impl Default for TextEncryption {
    fn default() -> Self {
        TextEncryption::Aes256Gcm
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageEncryption {
    Aes256Ctr,
}

impl ImageEncryption {
    const MARKER_AES256_CTR: &'static [u8] = b":AES256CTR:";

    fn marker(self) -> &'static [u8] {
        match self {
            ImageEncryption::Aes256Ctr => Self::MARKER_AES256_CTR,
        }
    }

    fn detect(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.starts_with(Self::MARKER_AES256_CTR) {
            return Some((Self::Aes256Ctr, Self::MARKER_AES256_CTR.len()));
        }
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryInfo {
    pub id: Uuid,
    pub title: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub folder: Option<String>,
}

impl EntryInfo {
    fn touch(&mut self) {
        self.updated_at = OffsetDateTime::now_utc();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub folder: Option<String>,
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
            folder: None,
        }
    }

    fn metadata(&self) -> EntryInfo {
        EntryInfo {
            id: self.id,
            title: self.title.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            folder: self.folder.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct UnlockResponse {
    pub entries: Vec<EntryInfo>,
    pub created: bool,
    pub last_saved: Option<String>,
    pub vault_root: String,
    pub text_encryption: TextEncryption,
    pub available_text_encryptions: Vec<TextEncryption>,
}

#[derive(Default)]
pub struct VaultManager {
    inner: Mutex<Option<UnlockedVault>>,
}

impl VaultManager {
    pub fn unlock(
        &self,
        passphrase: &str,
        metadata_path: PathBuf,
        preferred_encryption: Option<TextEncryption>,
    ) -> Result<UnlockResponse> {
        let root_path = metadata_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| metadata_path.clone());

        let entries_dir = root_path.join("entries");
        let attachments_dir = root_path.join("attachments");
        fs::create_dir_all(&entries_dir).context("failed to prepare entries directory")?;
        fs::create_dir_all(&attachments_dir).context("failed to prepare attachments directory")?;

        let available_methods = SUPPORTED_TEXT_ENCRYPTIONS.to_vec();

        if !metadata_path.exists() {
            let mut salt = [0u8; 16];
            OsRng.fill_bytes(&mut salt);
            let key = derive_key(passphrase, &salt)?;

            let text_encryption = preferred_encryption.unwrap_or_default();
            if !SUPPORTED_TEXT_ENCRYPTIONS.contains(&text_encryption) {
                return Err(anyhow!("unsupported text encryption method"));
            }

            let metadata = VaultMetadata {
                version: METADATA_VERSION,
                entries: Vec::new(),
                text_encryption,
            };
            let now = OffsetDateTime::now_utc();
            save_vault(&metadata_path, &salt, &key, &metadata, now)?;

            let unlocked = UnlockedVault {
                key,
                salt,
                metadata: Vec::new(),
                path: metadata_path,
                entries_dir,
                attachments_dir,
                text_encryption,
                last_saved: now,
            };

            *self.inner.lock() = Some(unlocked);

            return Ok(UnlockResponse {
                entries: Vec::new(),
                created: true,
                last_saved: Some(now.format(&Rfc3339).unwrap_or_default()),
                vault_root: display_path(&root_path),
                text_encryption,
                available_text_encryptions: available_methods,
            });
        }

        let stored = load_vault(&metadata_path)?;
        let salt_vec = general_purpose::STANDARD_NO_PAD
            .decode(&stored.salt)
            .context("invalid salt encoding")?;
        if salt_vec.len() != 16 {
            return Err(anyhow!("invalid salt length"));
        }
        let mut salt = [0u8; 16];
        salt.copy_from_slice(&salt_vec);

        let key = derive_key(passphrase, &salt)?;
        let metadata = decrypt_metadata(&stored, &key)?;
        let VaultMetadata {
            version,
            entries,
            text_encryption,
        } = metadata;
        if version != METADATA_VERSION {
            return Err(anyhow!("unsupported metadata version"));
        }
        if !SUPPORTED_TEXT_ENCRYPTIONS.contains(&text_encryption) {
            return Err(anyhow!("unsupported text encryption method"));
        }

        let last_saved = stored.updated_at.unwrap_or_else(OffsetDateTime::now_utc);

        let entries_clone = entries.clone();

        let unlocked = UnlockedVault {
            key,
            salt,
            metadata: entries,
            path: metadata_path,
            entries_dir,
            attachments_dir,
            text_encryption,
            last_saved,
        };

        *self.inner.lock() = Some(unlocked);

        Ok(UnlockResponse {
            entries: entries_clone,
            created: false,
            last_saved: stored.updated_at.and_then(|ts| ts.format(&Rfc3339).ok()),
            vault_root: display_path(&root_path),
            text_encryption,
            available_text_encryptions: available_methods,
        })
    }

    pub fn lock(&self) {
        *self.inner.lock() = None;
    }

    pub fn list(&self) -> Result<Vec<EntryInfo>> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        let mut entries = vault.metadata.clone();
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(entries)
    }

    pub fn load_entry(&self, id: Uuid) -> Result<Entry> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        let meta = vault
            .metadata
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
            .ok_or_else(|| anyhow!("entry not found"))?;
        let content = load_entry_content(
            &vault.entries_dir,
            &vault.key,
            vault.text_encryption,
            &meta.id,
        )?;
        Ok(Entry {
            id: meta.id,
            title: meta.title,
            content,
            created_at: meta.created_at,
            updated_at: meta.updated_at,
            folder: meta.folder,
        })
    }

    pub fn create_entry(&self, title: &str, content: &str) -> Result<Entry> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        let entry = Entry::new(title, content);
        save_entry_content(
            &vault.entries_dir,
            &vault.key,
            vault.text_encryption,
            &entry,
        )?;
        vault.metadata.push(entry.metadata());
        save_metadata(vault)?;
        Ok(entry)
    }

    pub fn update_entry(&self, entry: Entry) -> Result<Entry> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        let info = vault
            .metadata
            .iter_mut()
            .find(|item| item.id == entry.id)
            .ok_or_else(|| anyhow!("entry not found"))?;

        info.title = entry.title.clone();
        info.folder = entry.folder.clone();
        info.touch();

        let updated = Entry {
            id: entry.id,
            title: entry.title,
            content: entry.content,
            created_at: info.created_at,
            updated_at: info.updated_at,
            folder: info.folder.clone(),
        };

        save_entry_content(
            &vault.entries_dir,
            &vault.key,
            vault.text_encryption,
            &updated,
        )?;
        save_metadata(vault)?;
        Ok(updated)
    }

    pub fn delete_entry(&self, id: Uuid) -> Result<()> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;
        let len_before = vault.metadata.len();
        vault.metadata.retain(|entry| entry.id != id);
        if vault.metadata.len() == len_before {
            return Err(anyhow!("entry not found"));
        }

        let content_path = entry_file_path(&vault.entries_dir, &id);
        if content_path.exists() {
            fs::remove_file(&content_path).context("failed to remove entry file")?;
        }

        save_metadata(vault)?;
        Ok(())
    }

    pub fn export_plaintext(&self) -> Result<String> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        let mut entries = vault.metadata.clone();
        entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let mut lines = Vec::new();
        for info in entries.iter() {
            let content = load_entry_content(
                &vault.entries_dir,
                &vault.key,
                vault.text_encryption,
                &info.id,
            )?;
            lines.push(format!(
                "# {title}\n创建：{created}\n更新：{updated}\n\n{content}\n",
                title = info.title,
                created = info
                    .created_at
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| String::new()),
                updated = info
                    .updated_at
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| String::new()),
                content = content
            ));
        }
        Ok(lines.join("\n---\n\n"))
    }

    pub fn vault_root(&self) -> Result<PathBuf> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;
        Ok(vault
            .path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| vault.path.clone()))
    }

    pub fn store_image(&self, source: PathBuf) -> Result<String> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

        if !source.exists() {
            return Err(anyhow!("选定的图片不存在"));
        }

        let extension = source
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.trim_start_matches('.'))
            .filter(|ext| !ext.is_empty())
            .unwrap_or("bin");
        let (target_path, relative) = attachment_target(vault, extension)?;

        // 读取并加密图片
        let data = fs::read(&source).context("无法读取图片文件")?;
        let encrypted = encrypt_image_data(&vault.key, &data)?;
        fs::write(&target_path, encrypted).context("无法保存加密图片")?;

        Ok(display_path(&relative))
    }

    pub fn store_image_bytes(
        &self,
        name: Option<String>,
        mime: Option<String>,
        data: Vec<u8>,
    ) -> Result<String> {
        let mut guard = self.inner.lock();
        let vault = guard.as_mut().ok_or_else(|| anyhow!("vault is locked"))?;

        if data.is_empty() {
            return Err(anyhow!("粘贴的图像为空"));
        }

        let extension = infer_image_extension(name.as_deref(), mime.as_deref());
        let (target_path, relative) = attachment_target(vault, &extension)?;

        // 加密图片数据
        let encrypted = encrypt_image_data(&vault.key, &data)?;
        fs::write(&target_path, encrypted).context("无法写入加密图片数据")?;

        Ok(display_path(&relative))
    }

    pub fn decrypt_image(&self, path: &str) -> Result<Vec<u8>> {
        let guard = self.inner.lock();
        let vault = guard.as_ref().ok_or_else(|| anyhow!("vault is locked"))?;

        // 解析路径（可能是相对路径或绝对路径）
        let image_path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            let root = vault
                .path
                .parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| vault.path.clone());
            root.join(path.trim_start_matches('/').trim_start_matches('\\'))
        };

        if !image_path.exists() {
            return Err(anyhow!("图片文件不存在"));
        }

        let encrypted = fs::read(&image_path).context("无法读取图片文件")?;
        decrypt_image_data(&vault.key, &encrypted)
    }
}

fn save_metadata(vault: &mut UnlockedVault) -> Result<()> {
    vault.last_saved = OffsetDateTime::now_utc();
    let metadata = VaultMetadata {
        version: METADATA_VERSION,
        entries: vault.metadata.clone(),
        text_encryption: vault.text_encryption,
    };
    save_vault(
        &vault.path,
        &vault.salt,
        &vault.key,
        &metadata,
        vault.last_saved,
    )
}

struct UnlockedVault {
    key: [u8; 32],
    salt: [u8; 16],
    metadata: Vec<EntryInfo>,
    path: PathBuf,
    entries_dir: PathBuf,
    attachments_dir: PathBuf,
    text_encryption: TextEncryption,
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

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    version: u32,
    nonce: String,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct VaultMetadata {
    version: u32,
    entries: Vec<EntryInfo>,
    #[serde(default)]
    text_encryption: TextEncryption,
}

fn derive_key(passphrase: &str, salt: &[u8; 16]) -> Result<[u8; 32]> {
    let argon = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(32768, 2, 4, Some(32)).context("invalid argon2 parameters")?,
    );
    let mut key = [0u8; 32];
    argon
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .context("failed to derive key")?;
    Ok(key)
}

fn save_vault(
    path: &PathBuf,
    salt: &[u8; 16],
    key: &[u8; 32],
    metadata: &VaultMetadata,
    timestamp: OffsetDateTime,
) -> Result<()> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid key"))?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    #[allow(deprecated)]
    let nonce = Nonce::from_slice(&nonce_bytes);

    let payload = serde_json::to_vec(metadata).context("failed to serialize metadata")?;
    let ciphertext = cipher
        .encrypt(nonce, payload.as_ref())
        .map_err(|_| anyhow!("encryption failed"))?;

    let stored = StoredVault {
        version: VAULT_VERSION,
        salt: general_purpose::STANDARD_NO_PAD.encode(salt),
        nonce: general_purpose::STANDARD_NO_PAD.encode(nonce_bytes),
        ciphertext: general_purpose::STANDARD_NO_PAD.encode(ciphertext),
        updated_at: Some(timestamp),
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

fn decrypt_metadata(stored: &StoredVault, key: &[u8; 32]) -> Result<VaultMetadata> {
    let nonce_bytes = general_purpose::STANDARD_NO_PAD
        .decode(&stored.nonce)
        .context("invalid nonce encoding")?;
    if nonce_bytes.len() != 12 {
        return Err(anyhow!("invalid nonce length"));
    }

    let ciphertext = general_purpose::STANDARD_NO_PAD
        .decode(&stored.ciphertext)
        .context("invalid ciphertext encoding")?;

    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid key"))?;
    #[allow(deprecated)]
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| anyhow!("decryption failed"))?;

    let metadata: VaultMetadata = serde_json::from_slice(&plaintext).context("invalid metadata")?;
    Ok(metadata)
}

fn save_entry_content(
    entries_dir: &Path,
    key: &[u8; 32],
    method: TextEncryption,
    entry: &Entry,
) -> Result<()> {
    fs::create_dir_all(entries_dir).context("failed to create entries directory")?;
    let (nonce_bytes, ciphertext) = match method {
        TextEncryption::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid key"))?;
            let mut nonce_bytes = [0u8; 12];
            OsRng.fill_bytes(&mut nonce_bytes);
            #[allow(deprecated)]
            let nonce = Nonce::from_slice(&nonce_bytes);
            let ciphertext = cipher
                .encrypt(nonce, entry.content.as_bytes())
                .map_err(|_| anyhow!("encryption failed"))?;
            (nonce_bytes, ciphertext)
        }
    };

    let stored = StoredEntry {
        version: ENTRY_VERSION,
        nonce: general_purpose::STANDARD_NO_PAD.encode(nonce_bytes),
        ciphertext: general_purpose::STANDARD_NO_PAD.encode(ciphertext),
    };

    let serialized = serde_json::to_string_pretty(&stored).context("failed to serialize entry")?;
    let path = entry_file_path(entries_dir, &entry.id);
    fs::write(path, serialized).context("failed to store entry")
}

fn load_entry_content(
    entries_dir: &Path,
    key: &[u8; 32],
    method: TextEncryption,
    id: &Uuid,
) -> Result<String> {
    let path = entry_file_path(entries_dir, id);
    if !path.exists() {
        return Err(anyhow!("entry content missing"));
    }
    let content = fs::read_to_string(&path).context("failed to read entry")?;
    let stored: StoredEntry = serde_json::from_str(&content).context("failed to parse entry")?;
    if stored.version != ENTRY_VERSION {
        return Err(anyhow!("unsupported entry version"));
    }

    let nonce_bytes = general_purpose::STANDARD_NO_PAD
        .decode(&stored.nonce)
        .context("invalid nonce encoding")?;
    if nonce_bytes.len() != 12 {
        return Err(anyhow!("invalid nonce length"));
    }
    let ciphertext = general_purpose::STANDARD_NO_PAD
        .decode(&stored.ciphertext)
        .context("invalid ciphertext encoding")?;

    let plaintext = match method {
        TextEncryption::Aes256Gcm => {
            let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("invalid key"))?;
            #[allow(deprecated)]
            let nonce = Nonce::from_slice(&nonce_bytes);
            cipher
                .decrypt(nonce, ciphertext.as_ref())
                .map_err(|_| anyhow!("decryption failed"))?
        }
    };

    let content = String::from_utf8(plaintext).context("invalid entry content")?;
    Ok(content)
}

fn entry_file_path(entries_dir: &Path, id: &Uuid) -> PathBuf {
    let mut path = entries_dir.to_path_buf();
    path.push(format!("{id}.bin", id = id));
    path
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn vault_file_path(mut base: PathBuf) -> PathBuf {
    base.push("vault.json");
    base
}

fn attachment_target(vault: &UnlockedVault, extension: &str) -> Result<(PathBuf, PathBuf)> {
    let now = OffsetDateTime::now_utc();
    let year = now.year();
    let month: u8 = now.month().into();

    let mut target_dir = vault.attachments_dir.clone();
    target_dir.push(year.to_string());
    target_dir.push(format!("{:02}", month));
    fs::create_dir_all(&target_dir).context("failed to prepare attachment directory")?;

    let ext = if extension.is_empty() {
        "bin"
    } else {
        extension
    };
    let filename = format!("{id}.{ext}", id = Uuid::new_v4(), ext = ext);
    let target_path = target_dir.join(filename);

    let root = vault
        .path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| vault.path.clone());
    let relative = target_path
        .strip_prefix(&root)
        .unwrap_or(&target_path)
        .to_path_buf();

    Ok((target_path, relative))
}

fn infer_image_extension(name: Option<&str>, mime: Option<&str>) -> String {
    if let Some(name) = name {
        if let Some(ext) = Path::new(name)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.trim_start_matches('.'))
        {
            let lowered = ext.to_ascii_lowercase();
            if !lowered.is_empty() {
                return lowered;
            }
        }
    }

    if let Some(mime) = mime {
        match mime.to_ascii_lowercase().as_str() {
            "image/png" => return "png".into(),
            "image/jpeg" | "image/jpg" => return "jpg".into(),
            "image/gif" => return "gif".into(),
            "image/webp" => return "webp".into(),
            "image/bmp" => return "bmp".into(),
            "image/svg+xml" => return "svg".into(),
            _ => {}
        }
    }

    "bin".into()
}

// 使用 AES-256-CTR 加密图片
fn encrypt_image_data(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    // 生成随机 IV (16 字节，CTR 模式使用 128 位)
    let mut iv = [0u8; 16];
    OsRng.fill_bytes(&mut iv);

    let method = ImageEncryption::Aes256Ctr;
    let marker = method.marker();

    // 创建输出缓冲区：魔数 + 标记 + IV + 密文
    let mut encrypted =
        Vec::with_capacity(IMAGE_MAGIC_PREFIX.len() + marker.len() + iv.len() + data.len());
    encrypted.extend_from_slice(IMAGE_MAGIC_PREFIX);
    encrypted.extend_from_slice(marker);

    // 复制数据用于加密
    let mut buffer = data.to_vec();

    // 使用 AES-256-CTR 加密
    let mut cipher = Aes256Ctr::new(key.into(), &iv.into());
    cipher.apply_keystream(&mut buffer);

    encrypted.extend_from_slice(&iv);
    encrypted.extend_from_slice(&buffer);
    Ok(encrypted)
}

fn decrypt_image_data(key: &[u8; 32], encrypted: &[u8]) -> Result<Vec<u8>> {
    // 检查最小长度：魔数(8) + IV(16)
    if encrypted.len() < IMAGE_MAGIC_PREFIX.len() + 16 {
        return Err(anyhow!("invalid encrypted image: too short"));
    }

    // 检查魔数
    if &encrypted[..IMAGE_MAGIC_PREFIX.len()] != IMAGE_MAGIC_PREFIX {
        // 不是加密图片，可能是旧版本的明文图片
        return Ok(encrypted.to_vec());
    }

    let mut offset = IMAGE_MAGIC_PREFIX.len();
    let (method, marker_len) = ImageEncryption::detect(&encrypted[offset..])
        .map(|(m, len)| (m, len))
        .unwrap_or((ImageEncryption::Aes256Ctr, 0));
    offset += marker_len;

    if encrypted.len() < offset + 16 {
        return Err(anyhow!("invalid encrypted image: missing iv"));
    }

    // 提取 IV
    let iv_end = offset + 16;
    let mut iv = [0u8; 16];
    iv.copy_from_slice(&encrypted[offset..iv_end]);

    // 提取密文
    let ciphertext = &encrypted[iv_end..];
    let mut buffer = ciphertext.to_vec();

    // 按照标记的算法解密
    match method {
        ImageEncryption::Aes256Ctr => {
            let mut cipher = Aes256Ctr::new(key.into(), &iv.into());
            cipher.apply_keystream(&mut buffer);
        }
    }

    Ok(buffer)
}
