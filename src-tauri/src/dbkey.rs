// Original work © 2026 zhebuilds. AGPLv3 plus ADDITIONAL_TERMS.md.
//! The random key the library database is encrypted with, kept in `db.key` next to the database.
//! On Windows the file holds the key sealed with DPAPI for the signed-in user, so it opens only under
//! that account. Elsewhere it holds the bare key in a file only the owner can read; Android moves the
//! key into the Keystore in a later stage.
use std::path::Path;

use rand::TryRngCore;

use crate::error::AppResult;

pub const KEY_NAME: &str = "db.key";
pub const KEY_LEN: usize = 32;

/// The key in `file`, or a new one written there when the file does not exist yet.
pub fn load_or_create(file: &Path) -> AppResult<[u8; KEY_LEN]> {
    match std::fs::read(file) {
        Ok(stored) => {
            let key = platform::unseal(&stored)?;
            key.try_into().map_err(|_| "db.key does not hold a database key".into())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            let mut key = [0u8; KEY_LEN];
            rand::rngs::OsRng.try_fill_bytes(&mut key).map_err(|err| format!("random key: {err}"))?;
            write_new(file, &platform::seal(&key)?)?;
            Ok(key)
        }
        Err(err) => Err(err.into()),
    }
}

/// Written under a temporary name and renamed, so a crash never leaves half a key behind.
fn write_new(file: &Path, bytes: &[u8]) -> AppResult<()> {
    use std::io::Write;
    let partial = file.with_extension("key.partial");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut out = options.open(&partial)?;
    out.write_all(bytes)?;
    out.sync_all()?;
    drop(out);
    std::fs::rename(&partial, file)?;
    Ok(())
}

#[cfg(windows)]
mod platform {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB};

    use crate::error::AppResult;

    pub fn seal(plain: &[u8]) -> AppResult<Vec<u8>> {
        let input = blob(plain);
        let mut output = CRYPT_INTEGER_BLOB::default();
        // Current-user scope: without CRYPTPROTECT_LOCAL_MACHINE only this Windows account can unseal it.
        unsafe { CryptProtectData(&input, PCWSTR::null(), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
            .map_err(|err| format!("DPAPI protect: {err}"))?;
        Ok(take(output))
    }

    pub fn unseal(sealed: &[u8]) -> AppResult<Vec<u8>> {
        let input = blob(sealed);
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe { CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut output) }
            .map_err(|err| format!("DPAPI unprotect: {err}"))?;
        Ok(take(output))
    }

    fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB { cbData: bytes.len() as u32, pbData: bytes.as_ptr() as *mut u8 }
    }

    /// Copies DPAPI's output out and frees it, wiping it first because it may be the bare key.
    fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if output.pbData.is_null() {
            return Vec::new();
        }
        unsafe {
            let bytes = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
            let copy = bytes.to_vec();
            bytes.fill(0);
            LocalFree(Some(HLOCAL(output.pbData.cast())));
            copy
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use crate::error::AppResult;

    pub fn seal(plain: &[u8]) -> AppResult<Vec<u8>> {
        Ok(plain.to_vec())
    }

    pub fn unseal(sealed: &[u8]) -> AppResult<Vec<u8>> {
        Ok(sealed.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("tunepeerly-dbkey-{name}-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_new_key_is_kept_and_read_back() {
        let dir = temp_dir("roundtrip");
        let file = dir.join(KEY_NAME);
        let first = load_or_create(&file).unwrap();
        assert!(file.is_file());
        assert!(!dir.join("db.key.partial").exists());
        assert_eq!(load_or_create(&file).unwrap(), first);
        assert_ne!(load_or_create(&temp_dir("other").join(KEY_NAME)).unwrap(), first);
        // The file never holds the key in a form another account could read.
        #[cfg(windows)]
        assert!(!std::fs::read(&file).unwrap().windows(KEY_LEN).any(|part| part == first));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_key_file_of_the_wrong_size_is_refused() {
        let file = temp_dir("short").join(KEY_NAME);
        std::fs::write(&file, platform::seal(&[7u8; 5]).unwrap()).unwrap();
        assert!(load_or_create(&file).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_seals_and_unseals() {
        let secret = [0x5au8; KEY_LEN];
        let sealed = platform::seal(&secret).unwrap();
        assert_ne!(sealed.as_slice(), secret.as_slice());
        assert_eq!(platform::unseal(&sealed).unwrap(), secret);
        let mut broken = sealed.clone();
        let last = broken.len() - 1;
        broken[last] ^= 1;
        assert!(platform::unseal(&broken).is_err());
    }
}
