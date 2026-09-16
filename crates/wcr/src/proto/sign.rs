//! SPDX-License-Identifier: Apache-2.0
//! Ed25519 signatures for envelopes (on by default over internet).

use crate::error::{Error, Result};
use crate::proto::envelope::Envelope;
use crate::proto::flags::FLAG_SIGNED;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;
use zeroize::Zeroize;

const KEY_LEN: usize = 32;

#[derive(Clone)]
pub struct IdentityKeys {
    signing: SigningKey,
}

impl IdentityKeys {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        Self { signing }
    }

    pub fn from_bytes(seed: &[u8; KEY_LEN]) -> Self {
        Self {
            signing: SigningKey::from_bytes(seed),
        }
    }

    pub fn to_bytes(&self) -> [u8; KEY_LEN] {
        self.signing.to_bytes()
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing.verifying_key()
    }

    pub fn public_hex(&self) -> String {
        hex::encode(self.verifying_key().to_bytes())
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.verifying_key().to_bytes()
    }

    pub fn sign_envelope(&self, env: &mut Envelope) -> Result<()> {
        env.flags.set(FLAG_SIGNED, true);
        env.signature = None;
        let payload = env.signed_payload()?;
        let sig = self.signing.sign(&payload);
        env.signature = Some(sig.to_bytes());
        Ok(())
    }

    pub fn sign_bytes(&self, data: &[u8]) -> [u8; 64] {
        self.signing.sign(data).to_bytes()
    }
}

impl Drop for IdentityKeys {
    fn drop(&mut self) {
        let mut b = self.signing.to_bytes();
        b.zeroize();
    }
}

pub fn verify_envelope(env: &Envelope, pubkey: &VerifyingKey) -> Result<()> {
    if !env.flags.signed() {
        return Err(Error::protocol("envelope is not signed"));
    }
    let sig_bytes = env
        .signature
        .ok_or_else(|| Error::protocol("missing signature bytes"))?;
    let mut unsigned = env.clone();
    unsigned.signature = None;
    unsigned.flags.set(FLAG_SIGNED, true);
    let payload = unsigned.signed_payload()?;
    let sig = Signature::from_bytes(&sig_bytes);
    pubkey
        .verify(&payload, &sig)
        .map_err(|_| Error::protocol("signature check failed"))
}

pub fn verifying_key_from_bytes(bytes: &[u8]) -> Result<VerifyingKey> {
    if bytes.len() != 32 {
        return Err(Error::identity("public key must be 32 bytes"));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(bytes);
    VerifyingKey::from_bytes(&arr).map_err(|e| Error::identity(format!("bad public key: {e}")))
}

pub fn save_secret(path: &std::path::Path, keys: &IdentityKeys) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, hex::encode(keys.to_bytes()))?;
    Ok(())
}

pub fn load_secret(path: &std::path::Path) -> Result<IdentityKeys> {
    let s = std::fs::read_to_string(path)?;
    let bytes = hex::decode(s.trim()).map_err(|e| Error::identity(format!("bad key file: {e}")))?;
    if bytes.len() != KEY_LEN {
        return Err(Error::identity("key file has the wrong length"));
    }
    let mut seed = [0u8; KEY_LEN];
    seed.copy_from_slice(&bytes);
    Ok(IdentityKeys::from_bytes(&seed))
}

pub fn load_or_create(path: &std::path::Path) -> Result<IdentityKeys> {
    if path.exists() {
        load_secret(path)
    } else {
        let keys = IdentityKeys::generate();
        save_secret(path, &keys)?;
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::callsign::Callsign;
    use crate::proto::envelope::Envelope;
    use crate::proto::flags::Flags;

    #[test]
    fn sign_and_verify() {
        let keys = IdentityKeys::generate();
        let mut env = Envelope::new_msg(
            Callsign::parse("G4ABC").unwrap(),
            Callsign::parse("M0XYZ").unwrap(),
            1,
            b"signed".to_vec(),
            3,
            Flags::new(),
        )
        .unwrap();
        keys.sign_envelope(&mut env).unwrap();
        let bytes = env.encode().unwrap();
        let back = Envelope::decode(&bytes).unwrap();
        verify_envelope(&back, &keys.verifying_key()).unwrap();
    }
}
