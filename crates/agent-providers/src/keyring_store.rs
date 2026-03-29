use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use directories::ProjectDirs;
use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;
use tracing::warn;

use super::{
    AuthMode, OAuthToken, ProviderConfig, ProviderKind, KEYCHAIN_SERVICE,
    OPENAI_BROWSER_AUTH_ISSUER,
};
use agent_core::APP_NAME;

pub(crate) const KEYCHAIN_SECRET_SAFE_UTF16_UNITS: usize = 1024;
const SEGMENTED_SECRET_STORAGE_FORMAT: &str = "segmented_secret_v1";
const SEGMENTED_OAUTH_TOKEN_STORAGE_FORMAT: &str = "segmented_oauth_token_v1";
const FILE_SECRET_STORE_DIR_NAME: &str = "secrets";

fn oauth_refresh_locks() -> &'static std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>> {
    static LOCKS: OnceLock<std::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

pub(crate) fn oauth_refresh_lock_for(account: &str) -> Arc<Mutex<()>> {
    let mut map = oauth_refresh_locks().lock().expect("lock poisoned");
    map.entry(account.to_owned())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

pub fn keychain_account(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

pub fn store_api_key(provider_id: &str, api_key: &str) -> Result<String> {
    let _ = initialize_keyring();
    let account = keychain_account(provider_id);
    set_secret(&account, api_key)?;
    Ok(account)
}

pub fn store_oauth_token(provider_id: &str, token: &OAuthToken) -> Result<String> {
    let _ = initialize_keyring();
    let account = keychain_account(provider_id);
    store_oauth_token_for_account(&account, token)?;
    Ok(account)
}

pub fn load_api_key(account: &str) -> Result<String> {
    let _ = initialize_keyring();
    get_secret(account).context("failed to read API key from secret store")
}

pub fn load_oauth_token(account: &str) -> Result<OAuthToken> {
    let _ = initialize_keyring();
    let raw = get_secret(account)?;
    deserialize_oauth_token_secret(account, &raw, get_secret)
}

pub fn delete_secret(account: &str) -> Result<()> {
    let _ = initialize_keyring();
    if let Ok(raw_stored) = get_secret_raw(account) {
        if let Some(metadata) = parse_segmented_secret_metadata(&raw_stored) {
            delete_segmented_secret_entries(account, &metadata);
        }
        let raw =
            deserialize_secret_storage(account, &raw_stored, get_secret_raw).unwrap_or(raw_stored);
        if let Some(metadata) = parse_segmented_oauth_metadata(&raw) {
            delete_segmented_oauth_entries(account, &metadata);
        }
    }
    delete_secret_raw_entry(account)
}

pub fn keyring_available() -> bool {
    initialize_keyring().is_ok() && Entry::new(KEYCHAIN_SERVICE, "probe").is_ok()
}

pub(crate) fn store_oauth_token_for_account(account: &str, token: &OAuthToken) -> Result<()> {
    let existing_segments = get_secret(account)
        .ok()
        .and_then(|raw| parse_segmented_oauth_metadata(&raw));
    match serialize_oauth_token_secret(account, token)? {
        SerializedOAuthTokenSecret::Inline(raw) => {
            set_secret(account, &raw)?;
            if let Some(metadata) = existing_segments.as_ref() {
                delete_segmented_oauth_entries(account, metadata);
            }
            Ok(())
        }
        SerializedOAuthTokenSecret::Segmented(secret) => {
            for (segment_account, segment_value) in &secret.segments {
                set_secret(segment_account, segment_value)?;
            }
            set_secret(account, &secret.metadata_raw)?;
            if let Some(metadata) = existing_segments.as_ref() {
                if metadata.segment_id != secret.metadata.segment_id {
                    delete_segmented_oauth_entries(account, metadata);
                }
            }
            Ok(())
        }
    }
}

pub(crate) fn api_key_for(provider: &ProviderConfig) -> Result<String> {
    let account = provider
        .keychain_account
        .as_deref()
        .ok_or_else(|| anyhow!("provider '{}' is missing keychain metadata", provider.id))?;
    load_api_key(account)
}

pub(crate) fn uses_openai_api_key_exchange(provider: &ProviderConfig) -> bool {
    provider.kind == ProviderKind::OpenAiCompatible && is_openai_browser_oauth(provider)
}

pub(crate) fn is_openai_browser_oauth(provider: &ProviderConfig) -> bool {
    provider.auth_mode == AuthMode::OAuth
        && provider
            .oauth
            .as_ref()
            .is_some_and(|oauth| oauth.authorization_url.contains(OPENAI_BROWSER_AUTH_ISSUER))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SegmentedSecretMetadata {
    storage_format: String,
    segment_id: String,
    chunks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SegmentedOAuthTokenMetadata {
    storage_format: String,
    segment_id: String,
    access_token_chunks: usize,
    refresh_token_chunks: usize,
    id_token_chunks: usize,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    token_type: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    org_id: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    display_email: Option<String>,
    #[serde(default)]
    subscription_type: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SegmentedOAuthTokenSecret {
    metadata: SegmentedOAuthTokenMetadata,
    pub(crate) metadata_raw: String,
    pub(crate) segments: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub(crate) struct SegmentedSecret {
    metadata: SegmentedSecretMetadata,
    pub(crate) metadata_raw: String,
    pub(crate) segments: Vec<(String, String)>,
}

pub(crate) enum SerializedSecret {
    Inline(String),
    Segmented(SegmentedSecret),
}

#[allow(clippy::large_enum_variant)]
pub(crate) enum SerializedOAuthTokenSecret {
    Inline(String),
    Segmented(SegmentedOAuthTokenSecret),
}

pub(crate) fn serialize_secret_storage(account: &str, secret: &str) -> Result<SerializedSecret> {
    if secret_storage_units(secret) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS {
        return Ok(SerializedSecret::Inline(secret.to_string()));
    }

    let segment_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let chunks = split_secret_chunks(secret, KEYCHAIN_SECRET_SAFE_UTF16_UNITS);
    let metadata = SegmentedSecretMetadata {
        storage_format: SEGMENTED_SECRET_STORAGE_FORMAT.to_string(),
        segment_id: segment_id.clone(),
        chunks: chunks.len(),
    };
    let metadata_raw =
        serde_json::to_string(&metadata).context("failed to encode segmented secret metadata")?;
    if secret_storage_units(&metadata_raw) > KEYCHAIN_SECRET_SAFE_UTF16_UNITS {
        bail!("segmented secret metadata exceeds keychain storage limits");
    }

    let segments = chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            (
                secret_segment_account_with_base(account, &segment_id, index),
                chunk,
            )
        })
        .collect();
    Ok(SerializedSecret::Segmented(SegmentedSecret {
        metadata,
        metadata_raw,
        segments,
    }))
}

pub(crate) fn serialize_oauth_token_secret(
    account: &str,
    token: &OAuthToken,
) -> Result<SerializedOAuthTokenSecret> {
    let raw = serde_json::to_string(token).context("failed to encode OAuth token")?;
    if secret_storage_units(&raw) <= KEYCHAIN_SECRET_SAFE_UTF16_UNITS {
        return Ok(SerializedOAuthTokenSecret::Inline(raw));
    }

    let segment_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string();
    let access_chunks = split_secret_chunks(&token.access_token, KEYCHAIN_SECRET_SAFE_UTF16_UNITS);
    let refresh_chunks = token
        .refresh_token
        .as_deref()
        .map(|value| split_secret_chunks(value, KEYCHAIN_SECRET_SAFE_UTF16_UNITS))
        .unwrap_or_default();
    let id_chunks = token
        .id_token
        .as_deref()
        .map(|value| split_secret_chunks(value, KEYCHAIN_SECRET_SAFE_UTF16_UNITS))
        .unwrap_or_default();

    let metadata = SegmentedOAuthTokenMetadata {
        storage_format: SEGMENTED_OAUTH_TOKEN_STORAGE_FORMAT.to_string(),
        segment_id: segment_id.clone(),
        access_token_chunks: access_chunks.len(),
        refresh_token_chunks: refresh_chunks.len(),
        id_token_chunks: id_chunks.len(),
        expires_at: token.expires_at,
        token_type: token.token_type.clone(),
        scopes: token.scopes.clone(),
        account_id: token.account_id.clone(),
        user_id: token.user_id.clone(),
        org_id: token.org_id.clone(),
        project_id: token.project_id.clone(),
        display_email: token.display_email.clone(),
        subscription_type: token.subscription_type.clone(),
    };
    let metadata_raw =
        serde_json::to_string(&metadata).context("failed to encode OAuth token metadata")?;
    if secret_storage_units(&metadata_raw) > KEYCHAIN_SECRET_SAFE_UTF16_UNITS {
        bail!("OAuth token metadata exceeds keychain storage limits");
    }

    let mut segments = Vec::new();
    append_segment_entries(
        &mut segments,
        account,
        &segment_id,
        "access_token",
        access_chunks,
    );
    append_segment_entries(
        &mut segments,
        account,
        &segment_id,
        "refresh_token",
        refresh_chunks,
    );
    append_segment_entries(&mut segments, account, &segment_id, "id_token", id_chunks);

    Ok(SerializedOAuthTokenSecret::Segmented(
        SegmentedOAuthTokenSecret {
            metadata,
            metadata_raw,
            segments,
        },
    ))
}

pub(crate) fn deserialize_oauth_token_secret<F>(
    account: &str,
    raw: &str,
    mut load_segment: F,
) -> Result<OAuthToken>
where
    F: FnMut(&str) -> Result<String>,
{
    if let Some(metadata) = parse_segmented_oauth_metadata(raw) {
        return load_segmented_oauth_token(account, &metadata, &mut load_segment);
    }

    serde_json::from_str(raw).context("failed to decode OAuth token from keychain")
}

pub(crate) fn deserialize_secret_storage<F>(
    account: &str,
    raw: &str,
    mut load_segment: F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<String>,
{
    if let Some(metadata) = parse_segmented_secret_metadata(raw) {
        return load_segmented_secret(account, &metadata, &mut load_segment);
    }
    Ok(raw.to_string())
}

fn set_secret(account: &str, secret: &str) -> Result<()> {
    let existing_segments = get_secret_raw(account)
        .ok()
        .and_then(|raw| parse_segmented_secret_metadata(&raw));
    match serialize_secret_storage(account, secret)? {
        SerializedSecret::Inline(raw) => {
            set_secret_raw(account, &raw)?;
            if let Some(metadata) = existing_segments.as_ref() {
                delete_segmented_secret_entries(account, metadata);
            }
            Ok(())
        }
        SerializedSecret::Segmented(secret) => {
            for (segment_account, segment_value) in &secret.segments {
                set_secret_raw(segment_account, segment_value)?;
            }
            set_secret_raw(account, &secret.metadata_raw)?;
            if let Some(metadata) = existing_segments.as_ref() {
                if metadata.segment_id != secret.metadata.segment_id {
                    delete_segmented_secret_entries(account, metadata);
                }
            }
            Ok(())
        }
    }
}

fn get_secret(account: &str) -> Result<String> {
    let raw = get_secret_raw(account)?;
    deserialize_secret_storage(account, &raw, get_secret_raw)
}

fn set_secret_raw(account: &str, secret: &str) -> Result<()> {
    match set_keyring_secret_raw(account, secret) {
        Ok(()) => {
            let _ = delete_fallback_secret(account);
            Ok(())
        }
        Err(error) => {
            warn!(
                "keychain write failed for account '{}'; using file-backed secret fallback: {error}",
                account
            );
            write_fallback_secret(account, secret).with_context(|| {
                format!("failed to store secret in file fallback after keychain failure: {error}")
            })
        }
    }
}

fn get_secret_raw(account: &str) -> Result<String> {
    if fallback_secret_exists(account) {
        match read_fallback_secret(account) {
            Ok(secret) => return Ok(secret),
            Err(error) => warn!(
                "failed to read fallback secret for account '{}'; retrying keychain: {error}",
                account
            ),
        }
    }
    get_keyring_secret_raw(account)
}

fn parse_segmented_secret_metadata(raw: &str) -> Option<SegmentedSecretMetadata> {
    serde_json::from_str::<SegmentedSecretMetadata>(raw)
        .ok()
        .filter(|metadata| metadata.storage_format == SEGMENTED_SECRET_STORAGE_FORMAT)
}

fn parse_segmented_oauth_metadata(raw: &str) -> Option<SegmentedOAuthTokenMetadata> {
    serde_json::from_str::<SegmentedOAuthTokenMetadata>(raw)
        .ok()
        .filter(|metadata| metadata.storage_format == SEGMENTED_OAUTH_TOKEN_STORAGE_FORMAT)
}

fn load_segmented_secret<F>(
    account: &str,
    metadata: &SegmentedSecretMetadata,
    load_segment: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<String>,
{
    if metadata.chunks == 0 {
        bail!("segmented secret is missing chunks");
    }

    let mut value = String::new();
    for index in 0..metadata.chunks {
        let segment_account =
            secret_segment_account_with_base(account, &metadata.segment_id, index);
        value.push_str(&load_segment(&segment_account)?);
    }
    Ok(value)
}

fn load_segmented_oauth_token<F>(
    account: &str,
    metadata: &SegmentedOAuthTokenMetadata,
    load_segment: &mut F,
) -> Result<OAuthToken>
where
    F: FnMut(&str) -> Result<String>,
{
    if metadata.access_token_chunks == 0 {
        bail!("segmented OAuth token is missing access token chunks");
    }

    Ok(OAuthToken {
        access_token: load_segmented_secret_field(
            account,
            metadata,
            "access_token",
            metadata.access_token_chunks,
            load_segment,
        )?,
        refresh_token: if metadata.refresh_token_chunks == 0 {
            None
        } else {
            Some(load_segmented_secret_field(
                account,
                metadata,
                "refresh_token",
                metadata.refresh_token_chunks,
                load_segment,
            )?)
        },
        expires_at: metadata.expires_at,
        token_type: metadata.token_type.clone(),
        scopes: metadata.scopes.clone(),
        id_token: if metadata.id_token_chunks == 0 {
            None
        } else {
            Some(load_segmented_secret_field(
                account,
                metadata,
                "id_token",
                metadata.id_token_chunks,
                load_segment,
            )?)
        },
        account_id: metadata.account_id.clone(),
        user_id: metadata.user_id.clone(),
        org_id: metadata.org_id.clone(),
        project_id: metadata.project_id.clone(),
        display_email: metadata.display_email.clone(),
        subscription_type: metadata.subscription_type.clone(),
    })
}

fn load_segmented_secret_field<F>(
    account: &str,
    metadata: &SegmentedOAuthTokenMetadata,
    field: &str,
    chunk_count: usize,
    load_segment: &mut F,
) -> Result<String>
where
    F: FnMut(&str) -> Result<String>,
{
    let mut value = String::new();
    for index in 0..chunk_count {
        let segment_account =
            oauth_segment_account_with_base(account, &metadata.segment_id, field, index);
        value.push_str(&load_segment(&segment_account)?);
    }
    Ok(value)
}

fn append_segment_entries(
    entries: &mut Vec<(String, String)>,
    account: &str,
    segment_id: &str,
    field: &str,
    chunks: Vec<String>,
) {
    for (index, chunk) in chunks.into_iter().enumerate() {
        entries.push((
            oauth_segment_account_with_base(account, segment_id, field, index),
            chunk,
        ));
    }
}

fn secret_segment_account_with_base(account: &str, segment_id: &str, index: usize) -> String {
    format!("{account}:secret:{segment_id}:{index}")
}

fn oauth_segment_account_with_base(
    account: &str,
    segment_id: &str,
    field: &str,
    index: usize,
) -> String {
    format!("{account}:oauth:{segment_id}:{field}:{index}")
}

fn delete_segmented_secret_entries(account: &str, metadata: &SegmentedSecretMetadata) {
    for index in 0..metadata.chunks {
        let segment_account =
            secret_segment_account_with_base(account, &metadata.segment_id, index);
        let _ = delete_secret_raw_entry(&segment_account);
    }
}

fn delete_segmented_oauth_entries(account: &str, metadata: &SegmentedOAuthTokenMetadata) {
    delete_segmented_field_entries(
        account,
        &metadata.segment_id,
        "access_token",
        metadata.access_token_chunks,
    );
    delete_segmented_field_entries(
        account,
        &metadata.segment_id,
        "refresh_token",
        metadata.refresh_token_chunks,
    );
    delete_segmented_field_entries(
        account,
        &metadata.segment_id,
        "id_token",
        metadata.id_token_chunks,
    );
}

fn delete_segmented_field_entries(
    account: &str,
    segment_id: &str,
    field: &str,
    chunk_count: usize,
) {
    for index in 0..chunk_count {
        let segment_account = oauth_segment_account_with_base(account, segment_id, field, index);
        let _ = delete_secret_raw_entry(&segment_account);
    }
}

pub(crate) fn split_secret_chunks(secret: &str, max_units: usize) -> Vec<String> {
    assert!(max_units > 0);
    if secret.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut count = 0usize;
    for character in secret.chars() {
        let character_units = character.len_utf16();
        if !chunk.is_empty() && count + character_units > max_units {
            chunks.push(chunk);
            chunk = String::new();
            count = 0;
        }
        chunk.push(character);
        count += character_units;
        if count >= max_units {
            chunks.push(chunk);
            chunk = String::new();
            count = 0;
        }
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
}

pub(crate) fn secret_storage_units(secret: &str) -> usize {
    secret.encode_utf16().count()
}

fn initialize_keyring() -> Result<()> {
    static INIT: std::sync::OnceLock<std::result::Result<(), String>> = std::sync::OnceLock::new();

    let result = INIT.get_or_init(|| {
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
        {
            let probe = Entry::new(KEYCHAIN_SERVICE, "probe").map_err(|error| error.to_string())?;
            drop(probe);
            Ok(())
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        {
            Err("unsupported platform for configured keyring backend".to_string())
        }
    });

    result
        .as_ref()
        .map_err(|message| anyhow!(message.clone()))
        .map(|_| ())
}

fn set_keyring_secret_raw(account: &str, secret: &str) -> Result<()> {
    let entry = Entry::new(KEYCHAIN_SERVICE, account)?;
    entry
        .set_password(secret)
        .context("failed to store secret in keychain")
}

fn get_keyring_secret_raw(account: &str) -> Result<String> {
    let entry = Entry::new(KEYCHAIN_SERVICE, account)?;
    entry
        .get_password()
        .context("failed to read secret from keychain")
}

fn delete_secret_raw_entry(account: &str) -> Result<()> {
    let fallback_removed = delete_fallback_secret(account)?;
    match Entry::new(KEYCHAIN_SERVICE, account) {
        Ok(entry) => match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(error)
                if fallback_removed || is_missing_keyring_delete_error(&error.to_string()) =>
            {
                Ok(())
            }
            Err(error) => Err(anyhow!(error).context("failed to delete secret from keychain")),
        },
        Err(_error) if fallback_removed => Ok(()),
        Err(error) => Err(anyhow!(error).context("failed to open secret entry")),
    }
}

fn fallback_secret_exists(account: &str) -> bool {
    fallback_secret_path(account)
        .map(|path| path.exists())
        .unwrap_or(false)
}

fn write_fallback_secret(account: &str, secret: &str) -> Result<()> {
    let path = fallback_secret_path(account)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create fallback secret directory {}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, secret)
        .with_context(|| format!("failed to write fallback secret file {}", path.display()))
}

fn read_fallback_secret(account: &str) -> Result<String> {
    let path = fallback_secret_path(account)?;
    fs::read_to_string(&path)
        .with_context(|| format!("failed to read fallback secret file {}", path.display()))
}

fn delete_fallback_secret(account: &str) -> Result<bool> {
    let path = fallback_secret_path(account)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error)
            .with_context(|| format!("failed to delete fallback secret file {}", path.display())),
    }
}

fn fallback_secret_path(account: &str) -> Result<PathBuf> {
    Ok(fallback_secret_root()?.join(format!(
        "{}.secret",
        URL_SAFE_NO_PAD.encode(account.as_bytes())
    )))
}

fn fallback_secret_root() -> Result<PathBuf> {
    if let Some(dirs) = ProjectDirs::from("com", "NuclearAI", APP_NAME) {
        return Ok(dirs.data_local_dir().join(FILE_SECRET_STORE_DIR_NAME));
    }

    fallback_secret_root_from_env()
}

#[cfg(windows)]
fn fallback_secret_root_from_env() -> Result<PathBuf> {
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from).or_else(|| {
        user_profile
            .as_ref()
            .map(|path| path.join("AppData").join("Roaming"))
    });
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| {
            user_profile
                .as_ref()
                .map(|path| path.join("AppData").join("Local"))
        })
        .or(app_data.clone());
    local_app_data
        .map(|root| {
            root.join("NuclearAI")
                .join(APP_NAME)
                .join(FILE_SECRET_STORE_DIR_NAME)
        })
        .ok_or_else(|| anyhow!("failed to resolve fallback secret storage directory"))
}

#[cfg(not(windows))]
fn fallback_secret_root_from_env() -> Result<PathBuf> {
    if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from) {
        return Ok(xdg_data_home
            .join("NuclearAI")
            .join(APP_NAME)
            .join(FILE_SECRET_STORE_DIR_NAME));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            home.join(".local")
                .join("share")
                .join("NuclearAI")
                .join(APP_NAME)
                .join(FILE_SECRET_STORE_DIR_NAME)
        })
        .ok_or_else(|| anyhow!("failed to resolve fallback secret storage directory"))
}

fn is_missing_keyring_delete_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("no entry")
        || error.contains("not found")
        || error.contains("cannot find")
        || error.contains("element not found")
}
