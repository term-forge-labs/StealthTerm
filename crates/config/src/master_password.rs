use sha2::Sha256;
use rand::RngCore;
use zeroize::Zeroizing;

const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT_LEN: usize = 32;
const KEY_LEN: usize = 32;

pub struct MasterPassword {
    salt: Vec<u8>,
    derived_key: Zeroizing<Vec<u8>>,
}

impl MasterPassword {
    /// Derive an encryption key from the user-supplied master password
    pub fn derive_from_password(password: &str, salt: Option<Vec<u8>>) -> Result<Self, String> {
        let salt = match salt {
            Some(s) => s,
            None => {
                let mut s = vec![0u8; SALT_LEN];
                rand::rngs::OsRng.fill_bytes(&mut s);
                s
            }
        };

        let mut key = Zeroizing::new(vec![0u8; KEY_LEN]);
        pbkdf2::pbkdf2_hmac::<Sha256>(
            password.as_bytes(),
            &salt,
            PBKDF2_ITERATIONS,
            &mut *key,
        );

        Ok(Self {
            salt,
            derived_key: key,
        })
    }

    pub fn key(&self) -> &[u8] {
        &self.derived_key
    }

    pub fn salt(&self) -> &[u8] {
        &self.salt
    }
}
