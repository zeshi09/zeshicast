//! The master key for `zeshicast.db` column encryption (Q6.3).
//!
//! Looked up in this order:
//!
//! 1. `<config>/db.key`, 32 bytes, kept at 0600 (the fallback store, and
//!    what an export with history carries);
//! 2. the Secret Service (`org.freedesktop.secrets`: gnome-keyring,
//!    KeePassXC, KWallet), an item with `application = zeshicast`.
//!
//! A new key goes to the Secret Service when one answers without a prompt,
//! to the file otherwise. The database remembers that a key was set up (a
//! sealed check value in `meta`), so a key that went missing or a wrong one
//! is reported as such and no data is returned, instead of quietly starting
//! over with a new key.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::crypto::{Cipher, KEY_LEN, random_bytes};

pub(crate) const KEY_FILE: &str = "db.key";
const CHECK_PLAINTEXT: &[u8] = b"zeshicast key check v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyError {
    /// The database was encrypted, but no key is found, or the one found
    /// does not match. Data stays unreadable until the user clears it.
    Lost(String),
    /// The key store is there but cannot answer now (keyring locked, I/O).
    /// Nothing is cached, the next call tries again.
    Unavailable(String),
}

impl std::error::Error for KeyError {}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lost(reason) => write!(
                f,
                "clipboard history and snippets are encrypted and cannot be read: {reason}. \
                 Restore the key (~/.config/zeshicast/{KEY_FILE} or the keyring item \
                 \"zeshicast database key\"), or start over with \
                 `zeshicast --reset-encrypted-data`, which deletes them"
            ),
            Self::Unavailable(reason) => write!(f, "the database key is unavailable: {reason}"),
        }
    }
}

/// Where the database's key lives, as recorded in `meta.key_store`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyStore {
    File,
    SecretService,
}

impl KeyStore {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::SecretService => "secret-service",
        }
    }
}

fn cache() -> &'static Mutex<HashMap<PathBuf, Arc<Cipher>>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<Cipher>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Forget cached keys (after `--reset-encrypted-data`, and in tests).
pub(crate) fn forget(config_dir: &Path) {
    if let Ok(mut cache) = cache().lock() {
        cache.remove(config_dir);
    }
}

/// The cipher for `config_dir`, creating a key on first use. `check` reads
/// and writes the database's check value: `Some(blob)` when one is stored,
/// and `store` saves a new one together with where the key went.
pub(crate) fn cipher_for(
    config_dir: &Path,
    stored_check: Option<Vec<u8>>,
    store_check: impl FnOnce(&[u8], KeyStore) -> std::io::Result<()>,
) -> Result<Arc<Cipher>, KeyError> {
    if let Some(cipher) = cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(config_dir).cloned())
    {
        return Ok(cipher);
    }
    let found = match read_key_file(config_dir)? {
        Some(key) => Some((key, KeyStore::File)),
        None => secret_service::find(config_dir)?.map(|key| (key, KeyStore::SecretService)),
    };
    let cipher = match (found, stored_check) {
        (Some((key, _)), Some(check)) => {
            let cipher = Cipher::new(&key);
            if cipher.open(&check).ok().as_deref() != Some(CHECK_PLAINTEXT) {
                return Err(KeyError::Lost(
                    "the key found does not match the database".to_string(),
                ));
            }
            cipher
        }
        (Some((key, store)), None) => {
            // A key without encrypted data (a fresh database, or an imported
            // key file): adopt it.
            let cipher = Cipher::new(&key);
            let check = seal_check(&cipher)?;
            store_check(&check, store).map_err(|error| KeyError::Unavailable(error.to_string()))?;
            cipher
        }
        (None, Some(_)) => {
            return Err(KeyError::Lost("no key was found".to_string()));
        }
        (None, None) => {
            let key = random_bytes::<KEY_LEN>()
                .map_err(|error| KeyError::Unavailable(error.to_string()))?;
            let store = if secret_service::store(config_dir, &key) {
                KeyStore::SecretService
            } else {
                write_key_file(config_dir, &key)
                    .map_err(|error| KeyError::Unavailable(error.to_string()))?;
                KeyStore::File
            };
            let cipher = Cipher::new(&key);
            let check = seal_check(&cipher)?;
            store_check(&check, store).map_err(|error| KeyError::Unavailable(error.to_string()))?;
            cipher
        }
    };
    let cipher = Arc::new(cipher);
    if let Ok(mut cache) = cache().lock() {
        cache.insert(config_dir.to_path_buf(), Arc::clone(&cipher));
    }
    Ok(cipher)
}

fn seal_check(cipher: &Cipher) -> Result<Vec<u8>, KeyError> {
    cipher
        .seal(CHECK_PLAINTEXT)
        .map_err(|error| KeyError::Unavailable(error.to_string()))
}

fn read_key_file(config_dir: &Path) -> Result<Option<[u8; KEY_LEN]>, KeyError> {
    use std::os::unix::fs::PermissionsExt;
    let path = config_dir.join(KEY_FILE);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(KeyError::Unavailable(format!(
                "{}: {error}",
                path.display()
            )));
        }
    };
    let key: [u8; KEY_LEN] = bytes
        .try_into()
        .map_err(|_| KeyError::Lost(format!("{} is not a {KEY_LEN}-byte key", path.display())))?;
    // Someone loosened it (a copy, an unpacked archive): tighten it back.
    if let Ok(meta) = std::fs::metadata(&path)
        && meta.permissions().mode() & 0o077 != 0
        && let Err(error) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
    {
        log::warn!("could not restrict {} to 0600: {error}", path.display());
    }
    Ok(Some(key))
}

fn write_key_file(config_dir: &Path, key: &[u8; KEY_LEN]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::create_dir_all(config_dir)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(config_dir.join(KEY_FILE))?;
    file.write_all(key)?;
    file.sync_all()
}

/// The key bytes, for an export that carries history: the archive then
/// restores on a machine without this keyring.
pub(crate) fn export_key(config_dir: &Path) -> Option<[u8; KEY_LEN]> {
    match read_key_file(config_dir) {
        Ok(Some(key)) => Some(key),
        _ => secret_service::find(config_dir).ok().flatten(),
    }
}

/// Delete the key everywhere (after the data it protected was deleted).
pub(crate) fn delete_key(config_dir: &Path) {
    let _ = std::fs::remove_file(config_dir.join(KEY_FILE));
    secret_service::delete(config_dir);
    forget(config_dir);
}

#[cfg(all(feature = "gui", not(test)))]
mod secret_service {
    //! A minimal client of the freedesktop Secret Service API over gio:
    //! the `plain` session (the bus is local), search, read, create without
    //! a prompt. A locked keyring or a prompt counts as "not available now".

    use super::{KEY_LEN, KeyError};
    use glib::variant::{ObjectPath, ToVariant};
    use std::collections::HashMap;
    use std::path::Path;

    const BUS_NAME: &str = "org.freedesktop.secrets";
    const SERVICE_PATH: &str = "/org/freedesktop/secrets";
    const SERVICE: &str = "org.freedesktop.Secret.Service";
    const TIMEOUT_MS: i32 = 3000;

    fn attributes(config_dir: &Path) -> HashMap<String, String> {
        HashMap::from([
            ("application".to_string(), "zeshicast".to_string()),
            ("purpose".to_string(), "database-key".to_string()),
            ("config-dir".to_string(), config_dir.display().to_string()),
        ])
    }

    struct Session {
        bus: gio::DBusConnection,
        path: String,
    }

    impl Session {
        fn open() -> Option<Self> {
            let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()?;
            let reply = call(
                &bus,
                SERVICE_PATH,
                SERVICE,
                "OpenSession",
                ("plain", "".to_variant()).to_variant(),
            )?;
            let path = reply.child_value(1).str()?.to_string();
            Some(Self { bus, path })
        }

        fn object_path(&self) -> Option<ObjectPath> {
            ObjectPath::try_from(self.path.clone()).ok()
        }
    }

    impl Drop for Session {
        fn drop(&mut self) {
            let _ = call(
                &self.bus,
                &self.path,
                "org.freedesktop.Secret.Session",
                "Close",
                ().to_variant(),
            );
        }
    }

    fn call(
        bus: &gio::DBusConnection,
        path: &str,
        interface: &str,
        method: &str,
        params: glib::Variant,
    ) -> Option<glib::Variant> {
        bus.call_sync(
            Some(BUS_NAME),
            path,
            interface,
            method,
            Some(&params),
            None,
            gio::DBusCallFlags::NONE,
            TIMEOUT_MS,
            gio::Cancellable::NONE,
        )
        .map_err(|error| log::debug!("secret service {method}: {error}"))
        .ok()
    }

    fn paths(array: &glib::Variant) -> Vec<String> {
        (0..array.n_children())
            .filter_map(|index| array.child_value(index).str().map(str::to_string))
            .collect()
    }

    /// The stored key, `Ok(None)` when there is no item (or no service).
    pub(super) fn find(config_dir: &Path) -> Result<Option<[u8; KEY_LEN]>, KeyError> {
        let Some(session) = Session::open() else {
            return Ok(None);
        };
        let Some(found) = call(
            &session.bus,
            SERVICE_PATH,
            SERVICE,
            "SearchItems",
            (attributes(config_dir),).to_variant(),
        ) else {
            return Ok(None);
        };
        let unlocked = paths(&found.child_value(0));
        let locked = paths(&found.child_value(1));
        let Some(item) = unlocked.first() else {
            if locked.is_empty() {
                return Ok(None);
            }
            return Err(KeyError::Unavailable(
                "the keyring holding it is locked; unlock it and try again".to_string(),
            ));
        };
        let items: Vec<ObjectPath> = ObjectPath::try_from(item.clone()).into_iter().collect();
        let Some(session_path) = session.object_path() else {
            return Ok(None);
        };
        let Some(secrets) = call(
            &session.bus,
            SERVICE_PATH,
            SERVICE,
            "GetSecrets",
            (items, session_path).to_variant(),
        ) else {
            return Err(KeyError::Unavailable(
                "the keyring did not return the key".to_string(),
            ));
        };
        let dict = secrets.child_value(0);
        if dict.n_children() == 0 {
            return Ok(None);
        }
        // a{o(oayays)}: the secret struct's third member is the value.
        let value = dict.child_value(0).child_value(1).child_value(2);
        let bytes: Vec<u8> = value
            .fixed_array::<u8>()
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        let key: [u8; KEY_LEN] = bytes.try_into().map_err(|_| {
            KeyError::Lost("the keyring item does not hold a 32-byte key".to_string())
        })?;
        Ok(Some(key))
    }

    /// Store a new key in the default collection; `false` when that needs a
    /// prompt or there is no service, so the caller uses the file.
    pub(super) fn store(config_dir: &Path, key: &[u8; KEY_LEN]) -> bool {
        let Some(session) = Session::open() else {
            return false;
        };
        let Some(alias) = call(
            &session.bus,
            SERVICE_PATH,
            SERVICE,
            "ReadAlias",
            ("default",).to_variant(),
        ) else {
            return false;
        };
        let collection = match alias.child_value(0).str() {
            Some(path) if path != "/" => path.to_string(),
            _ => return false,
        };
        let Some(session_path) = session.object_path() else {
            return false;
        };
        let properties: HashMap<String, glib::Variant> = HashMap::from([
            (
                "org.freedesktop.Secret.Item.Label".to_string(),
                "zeshicast database key".to_variant(),
            ),
            (
                "org.freedesktop.Secret.Item.Attributes".to_string(),
                attributes(config_dir).to_variant(),
            ),
        ]);
        let secret = (
            session_path,
            Vec::<u8>::new(),
            key.to_vec(),
            "application/octet-stream",
        );
        let Some(created) = call(
            &session.bus,
            &collection,
            "org.freedesktop.Secret.Collection",
            "CreateItem",
            (properties, secret, true).to_variant(),
        ) else {
            return false;
        };
        // A prompt means the collection is locked: do not wait for a user.
        created.child_value(1).str() == Some("/")
            && created.child_value(0).str().is_some_and(|item| item != "/")
    }

    pub(super) fn delete(config_dir: &Path) {
        let Some(session) = Session::open() else {
            return;
        };
        let Some(found) = call(
            &session.bus,
            SERVICE_PATH,
            SERVICE,
            "SearchItems",
            (attributes(config_dir),).to_variant(),
        ) else {
            return;
        };
        for item in paths(&found.child_value(0)) {
            let _ = call(
                &session.bus,
                &item,
                "org.freedesktop.Secret.Item",
                "Delete",
                ().to_variant(),
            );
        }
    }
}

/// Builds without gio (the headless CLI) and tests use the key file only.
#[cfg(not(all(feature = "gui", not(test))))]
mod secret_service {
    use super::{KEY_LEN, KeyError};
    use std::path::Path;

    pub(super) fn find(_config_dir: &Path) -> Result<Option<[u8; KEY_LEN]>, KeyError> {
        Ok(None)
    }

    pub(super) fn store(_config_dir: &Path, _key: &[u8; KEY_LEN]) -> bool {
        false
    }

    pub(super) fn delete(_config_dir: &Path) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("zeshicast-key-{name}-"))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn a_new_key_file_is_0600_and_found_again() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("new");
        let mut stored = None;
        let cipher = cipher_for(dir.path(), None, |check, store| {
            stored = Some((check.to_vec(), store));
            Ok(())
        })
        .unwrap();
        let (check, store) = stored.expect("a check value was saved");
        assert_eq!(store, KeyStore::File);
        let mode = std::fs::metadata(dir.path().join(KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        forget(dir.path());
        let again = cipher_for(dir.path(), Some(check), |_, _| panic!("already set up")).unwrap();
        let sealed = cipher.seal(b"x").unwrap();
        assert_eq!(again.open(&sealed).unwrap(), b"x");
    }

    #[test]
    fn a_missing_or_wrong_key_fails_closed() {
        let dir = temp("lost");
        let mut check = Vec::new();
        cipher_for(dir.path(), None, |value, _| {
            check = value.to_vec();
            Ok(())
        })
        .unwrap();

        forget(dir.path());
        std::fs::remove_file(dir.path().join(KEY_FILE)).unwrap();
        let error = cipher_for(dir.path(), Some(check.clone()), |_, _| {
            panic!("must not set up a new key over encrypted data")
        })
        .err()
        .unwrap();
        assert!(matches!(error, KeyError::Lost(_)));
        assert!(error.to_string().contains("--reset-encrypted-data"));
        assert!(!dir.path().join(KEY_FILE).exists(), "no new key was made");

        std::fs::write(dir.path().join(KEY_FILE), [9u8; KEY_LEN]).unwrap();
        let error = cipher_for(dir.path(), Some(check), |_, _| unreachable!())
            .err()
            .unwrap();
        assert!(error.to_string().contains("does not match"), "{error}");
    }

    #[test]
    fn a_loosened_key_file_is_tightened() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("loose");
        let path = dir.path().join(KEY_FILE);
        std::fs::write(&path, [1u8; KEY_LEN]).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_key_file(dir.path()).unwrap().is_some());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
