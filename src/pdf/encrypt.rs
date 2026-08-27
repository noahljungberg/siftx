//! PDF encryption and decryption (AV1-AV4).
//!
//! Implements the standard security handler for PDF encryption:
//! - AV1: RC4 40/128-bit and AES 128/256-bit decryption
//! - AV2: User password authentication
//! - AV3: Owner password authentication
//! - AV4: Permission checking
//! Per ISO 32000-2 §7.6.

use super::object::PdfObject;
use crate::core::{Error, Result};

use aes::Aes128;
use aes::Aes256;
use aes::cipher::block_padding::NoPadding;
use aes::cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use md5::{Digest as Md5Digest, Md5};
use sha2::{Sha256, Sha384, Sha512};

type Aes128CbcDec = cbc::Decryptor<Aes128>;
type Aes128CbcEnc = cbc::Encryptor<Aes128>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// AV1: Encryption algorithm variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// V1: RC4 with 40-bit key
    Rc4_40,
    /// V2: RC4 with variable key length (typically 128-bit)
    Rc4_128,
    /// V4: AES-128 in CBC mode
    Aes128,
    /// V5: AES-256 in CBC mode
    Aes256,
}

/// AV4: Permission flags (from /P value).
#[derive(Debug, Clone, Copy)]
pub struct PdfPermissions {
    /// Raw /P value (32-bit signed integer).
    pub raw: i32,
}

impl PdfPermissions {
    /// Bit 3: Print the document.
    pub fn can_print(&self) -> bool {
        self.raw & (1 << 2) != 0
    }

    /// Bit 4: Modify the contents.
    pub fn can_modify(&self) -> bool {
        self.raw & (1 << 3) != 0
    }

    /// Bit 5: Copy or extract text/graphics.
    pub fn can_copy(&self) -> bool {
        self.raw & (1 << 4) != 0
    }

    /// Bit 6: Add or modify annotations, fill form fields.
    pub fn can_annotate(&self) -> bool {
        self.raw & (1 << 5) != 0
    }

    /// Bit 9: Fill in form fields (even if bit 6 is clear).
    pub fn can_fill_forms(&self) -> bool {
        self.raw & (1 << 8) != 0
    }

    /// Bit 10: Extract text/graphics for accessibility.
    pub fn can_extract_accessibility(&self) -> bool {
        self.raw & (1 << 9) != 0
    }

    /// Bit 11: Assemble the document (insert, rotate, delete pages).
    pub fn can_assemble(&self) -> bool {
        self.raw & (1 << 10) != 0
    }

    /// Bit 12: Print high quality.
    pub fn can_print_high_quality(&self) -> bool {
        self.raw & (1 << 11) != 0
    }
}

/// AV1: The standard security handler.
///
/// Holds all encryption parameters and the derived file encryption key
/// after successful authentication.
#[derive(Debug, Clone)]
pub struct SecurityHandler {
    /// Encryption algorithm.
    pub algorithm: EncryptionAlgorithm,
    /// Key length in bytes.
    pub key_length: usize,
    /// /R revision (2-6).
    pub revision: u32,
    /// /O owner hash (32 or 48 bytes).
    pub owner_hash: Vec<u8>,
    /// /U user hash (32 or 48 bytes).
    pub user_hash: Vec<u8>,
    /// /P permissions value.
    pub permissions: i32,
    /// First element of the /ID array from the trailer.
    pub file_id: Vec<u8>,
    /// /EncryptMetadata (default true).
    pub encrypt_metadata: bool,
    /// /OE (R6 only, 32 bytes) - owner encryption key.
    pub oe: Option<Vec<u8>>,
    /// /UE (R6 only, 32 bytes) - user encryption key.
    pub ue: Option<Vec<u8>>,
    /// /Perms (R6 only, 16 bytes) - encrypted permission validation.
    pub perms: Option<Vec<u8>>,
    /// Derived file encryption key (set after successful authentication).
    encryption_key: Option<Vec<u8>>,
}

/// Standard PDF password padding (32 bytes, per ISO 32000-1 Table 3.19).
const PDF_PASSWORD_PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

impl SecurityHandler {
    /// Parse a SecurityHandler from the /Encrypt dictionary and trailer.
    pub fn from_encrypt_dict(encrypt: &PdfObject, trailer: &PdfObject) -> Result<Self> {
        let v = encrypt.dict_get(b"V").and_then(|v| v.as_int()).unwrap_or(0) as u32;
        let r = encrypt.dict_get(b"R").and_then(|r| r.as_int()).unwrap_or(0) as u32;
        let length_bits = encrypt
            .dict_get(b"Length")
            .and_then(|l| l.as_int())
            .unwrap_or(40) as usize;
        let key_length = length_bits / 8;

        let algorithm = match (v, r) {
            (1, _) => EncryptionAlgorithm::Rc4_40,
            (2, _) => EncryptionAlgorithm::Rc4_128,
            (4, _) => EncryptionAlgorithm::Aes128,
            (5, _) => EncryptionAlgorithm::Aes256,
            _ => return Err(Error::Unsupported(format!("encryption V={v} R={r}"))),
        };

        let key_length = match algorithm {
            EncryptionAlgorithm::Rc4_40 => 5,
            EncryptionAlgorithm::Aes256 => 32,
            _ => key_length.max(5).min(16),
        };

        let owner_hash = encrypt
            .dict_get(b"O")
            .and_then(|o| o.as_string())
            .unwrap_or(&[])
            .to_vec();

        let user_hash = encrypt
            .dict_get(b"U")
            .and_then(|u| u.as_string())
            .unwrap_or(&[])
            .to_vec();

        let permissions = encrypt.dict_get(b"P").and_then(|p| p.as_int()).unwrap_or(0) as i32;

        let encrypt_metadata = encrypt
            .dict_get(b"EncryptMetadata")
            .and_then(|e| e.as_bool())
            .unwrap_or(true);

        // Extract file ID from trailer /ID array
        let file_id = trailer
            .dict_get(b"ID")
            .and_then(|id| id.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.as_string())
            .unwrap_or(&[])
            .to_vec();

        // R6 fields
        let oe = encrypt
            .dict_get(b"OE")
            .and_then(|o| o.as_string())
            .map(|s| s.to_vec());
        let ue = encrypt
            .dict_get(b"UE")
            .and_then(|u| u.as_string())
            .map(|s| s.to_vec());
        let perms = encrypt
            .dict_get(b"Perms")
            .and_then(|p| p.as_string())
            .map(|s| s.to_vec());

        Ok(SecurityHandler {
            algorithm,
            key_length,
            revision: r,
            owner_hash,
            user_hash,
            permissions,
            file_id,
            encrypt_metadata,
            oe,
            ue,
            perms,
            encryption_key: None,
        })
    }

    /// AV4: Get permissions.
    pub fn permissions_flags(&self) -> PdfPermissions {
        PdfPermissions {
            raw: self.permissions,
        }
    }

    /// Check if a file encryption key has been derived (authentication succeeded).
    pub fn is_authenticated(&self) -> bool {
        self.encryption_key.is_some()
    }

    // --- AV2: User password authentication ---

    /// AV2: Authenticate with the user password.
    ///
    /// Returns true if authentication succeeds and the encryption key is derived.
    pub fn authenticate_user(&mut self, password: &[u8]) -> bool {
        if self.revision <= 4 {
            self.authenticate_user_r2_r4(password)
        } else {
            self.authenticate_user_r6(password)
        }
    }

    /// R2-R4 user authentication (Algorithm 4/5).
    fn authenticate_user_r2_r4(&mut self, password: &[u8]) -> bool {
        let key = self.compute_encryption_key_r2_r4(password);
        let computed_u = self.compute_user_hash_r2_r4(&key);

        let matches = if self.revision == 2 {
            // R2: compare full 32 bytes
            computed_u == self.user_hash
        } else {
            // R3/R4: compare first 16 bytes only
            let len = 16.min(computed_u.len()).min(self.user_hash.len());
            computed_u[..len] == self.user_hash[..len]
        };

        if matches {
            self.encryption_key = Some(key);
        }
        matches
    }

    /// R5/R6 user authentication (Algorithm 11 / ISO 32000-2 §7.6.4.4.10).
    ///
    /// Only the FIRST 48 bytes of /U are meaningful (32 hash + 8 validation
    /// salt + 8 key salt) - Acrobat routinely writes longer strings (127
    /// bytes observed in the wild) and the tail must be ignored, not
    /// compared. The password hash is [`hash_r6`]: plain SHA-256 for the
    /// deprecated R5, the Algorithm 2.B hardening loop for R6.
    fn authenticate_user_r6(&mut self, password: &[u8]) -> bool {
        if self.user_hash.len() < 48 {
            return false;
        }
        let u: [u8; 48] = match self.user_hash[..48].try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let password = truncate_password(password);

        // Validation: hash(password, validation salt = U[32..40], no udata)
        let hash = hash_r6(self.revision, password, &u[32..40], &[]);
        if hash[..32] != u[..32] {
            return false;
        }

        // Derive the file key from /UE: intermediate key from the key salt
        // (U[40..48]), then AES-256-CBC-decrypt the first 32 bytes of /UE
        // with a zero IV.
        if let Some(ref ue) = self.ue {
            if ue.len() >= 32 {
                let ikey = hash_r6(self.revision, password, &u[40..48], &[]);
                let iv = [0u8; 16];
                if let Some(decrypted) = aes256_cbc_decrypt(&ikey[..32], &iv, &ue[..32]) {
                    self.encryption_key = Some(decrypted[..32.min(decrypted.len())].to_vec());
                    return true;
                }
            }
        }

        false
    }

    // --- AV3: Owner password authentication ---

    /// AV3: Authenticate with the owner password.
    ///
    /// Returns true if authentication succeeds and the encryption key is derived.
    pub fn authenticate_owner(&mut self, password: &[u8]) -> bool {
        if self.revision <= 4 {
            self.authenticate_owner_r2_r4(password)
        } else {
            self.authenticate_owner_r6(password)
        }
    }

    /// R2-R4 owner authentication (Algorithm 7).
    ///
    /// Derives the owner key from the owner password, uses it to decrypt /O
    /// to get the user password, then authenticates as user.
    fn authenticate_owner_r2_r4(&mut self, password: &[u8]) -> bool {
        let padded = pad_password(password);

        // MD5 hash of padded password
        let mut hash = Md5::digest(&padded).to_vec();

        // For R3/R4: iterate MD5 50 times
        if self.revision >= 3 {
            for _ in 0..50 {
                hash = Md5::digest(&hash[..self.key_length]).to_vec();
            }
        }

        let owner_key = hash[..self.key_length].to_vec();

        // Decrypt /O to get user password
        let user_password = if self.revision == 2 {
            rc4_crypt(&owner_key, &self.owner_hash)
        } else {
            // R3/R4: 20 iterations of RC4 with modified keys (reverse order)
            let mut data = self.owner_hash.clone();
            for i in (0..=19).rev() {
                let mut modified_key = owner_key.clone();
                for byte in &mut modified_key {
                    *byte ^= i;
                }
                data = rc4_crypt(&modified_key, &data);
            }
            data
        };

        // Now authenticate as user with the recovered password
        self.authenticate_user(&user_password)
    }

    /// R5/R6 owner authentication (Algorithm 12 / ISO 32000-2 §7.6.4.4.11).
    ///
    /// Same 48-byte clamp as the user path - and crucially the udata mixed
    /// into the hash is the first 48 bytes of /U, NOT the whole (possibly
    /// over-long) string: hashing all 127 stored bytes made every real
    /// Acrobat AES-256 file fail owner auth.
    fn authenticate_owner_r6(&mut self, password: &[u8]) -> bool {
        if self.owner_hash.len() < 48 || self.user_hash.len() < 48 {
            return false;
        }
        let o: [u8; 48] = match self.owner_hash[..48].try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let u: [u8; 48] = match self.user_hash[..48].try_into() {
            Ok(a) => a,
            Err(_) => return false,
        };
        let password = truncate_password(password);

        // Validation: hash(password, validation salt = O[32..40], udata = U[0..48])
        let hash = hash_r6(self.revision, password, &o[32..40], &u);
        if hash[..32] != o[..32] {
            return false;
        }

        // Derive the file key from /OE (key salt = O[40..48], udata = U[0..48])
        if let Some(ref oe) = self.oe {
            if oe.len() >= 32 {
                let ikey = hash_r6(self.revision, password, &o[40..48], &u);
                let iv = [0u8; 16];
                if let Some(decrypted) = aes256_cbc_decrypt(&ikey[..32], &iv, &oe[..32]) {
                    self.encryption_key = Some(decrypted[..32.min(decrypted.len())].to_vec());
                    return true;
                }
            }
        }

        false
    }

    /// Try empty password first (most common for permission-only encryption).
    pub fn try_empty_password(&mut self) -> bool {
        self.authenticate_user(b"") || self.authenticate_owner(b"")
    }

    // --- AV1: Decryption ---

    /// AV1: Decrypt stream data for a specific object.
    pub fn decrypt_stream(&self, obj_num: u32, gen_num: u16, data: &[u8]) -> Result<Vec<u8>> {
        let key = self
            .encryption_key
            .as_ref()
            .ok_or_else(|| Error::Format("not authenticated".into()))?;

        match self.algorithm {
            EncryptionAlgorithm::Rc4_40 | EncryptionAlgorithm::Rc4_128 => {
                let obj_key = derive_object_key(key, obj_num, gen_num, self.key_length, false);
                Ok(rc4_crypt(&obj_key, data))
            }
            EncryptionAlgorithm::Aes128 => {
                let obj_key = derive_object_key(key, obj_num, gen_num, self.key_length, true);
                aes128_cbc_decrypt_with_iv(&obj_key, data)
            }
            EncryptionAlgorithm::Aes256 => {
                // AES-256: use file key directly (no per-object key derivation)
                aes256_cbc_decrypt_with_iv(key, data)
            }
        }
    }

    /// AV1: Decrypt string data for a specific object.
    pub fn decrypt_string(&self, obj_num: u32, gen_num: u16, data: &[u8]) -> Result<Vec<u8>> {
        // Same algorithm as stream decryption
        self.decrypt_stream(obj_num, gen_num, data)
    }

    // --- Internal key derivation ---

    /// Compute the file encryption key for R2-R4 (Algorithm 2).
    fn compute_encryption_key_r2_r4(&self, password: &[u8]) -> Vec<u8> {
        let padded = pad_password(password);

        let mut hasher = Md5::new();
        hasher.update(&padded);
        hasher.update(&self.owner_hash);

        // Permissions as little-endian 4-byte value
        let p_bytes = (self.permissions as u32).to_le_bytes();
        hasher.update(p_bytes);

        hasher.update(&self.file_id);

        // R4 with EncryptMetadata=false: include 4 bytes of 0xFF
        if self.revision >= 4 && !self.encrypt_metadata {
            hasher.update([0xFF, 0xFF, 0xFF, 0xFF]);
        }

        let mut hash = hasher.finalize().to_vec();

        // R3/R4: iterate MD5 50 times
        if self.revision >= 3 {
            for _ in 0..50 {
                hash = Md5::digest(&hash[..self.key_length]).to_vec();
            }
        }

        hash.truncate(self.key_length);

        hash
    }

    /// Compute expected /U value for R2-R4 (Algorithm 4/5).
    fn compute_user_hash_r2_r4(&self, key: &[u8]) -> Vec<u8> {
        if self.revision == 2 {
            // Algorithm 4: RC4-encrypt the padding
            rc4_crypt(key, &PDF_PASSWORD_PADDING)
        } else {
            // Algorithm 5: MD5(padding + file_id), then 20 RC4 iterations
            let mut hasher = Md5::new();
            hasher.update(PDF_PASSWORD_PADDING);
            hasher.update(&self.file_id);
            let mut hash = hasher.finalize().to_vec();

            hash = rc4_crypt(key, &hash);
            for i in 1..=19 {
                let mut modified_key = key.to_vec();
                for byte in &mut modified_key {
                    *byte ^= i;
                }
                hash = rc4_crypt(&modified_key, &hash);
            }

            // Pad to 32 bytes with arbitrary data
            hash.resize(32, 0);
            hash
        }
    }
}

// --- RC4 implementation ---

/// RC4 encrypt/decrypt (symmetric - same operation for both).
fn rc4_crypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }

    // Key-scheduling algorithm (KSA)
    let mut s: Vec<u8> = (0..=255).map(|i| i as u8).collect();
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }

    // Pseudo-random generation algorithm (PRGA)
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    let mut result = Vec::with_capacity(data.len());

    for &byte in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[s[i as usize].wrapping_add(s[j as usize]) as usize];
        result.push(byte ^ k);
    }

    result
}

// --- AES helpers ---

/// Derive per-object encryption key (Algorithm 1 for R2-R4, modified for AES).
fn derive_object_key(
    file_key: &[u8],
    obj_num: u32,
    gen_num: u16,
    key_length: usize,
    is_aes: bool,
) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(file_key);

    // Object number as 3 LE bytes
    let obj_bytes = obj_num.to_le_bytes();
    hasher.update(&obj_bytes[..3]);

    // Generation number as 2 LE bytes
    let gen_bytes = gen_num.to_le_bytes();
    hasher.update(&gen_bytes[..2]);

    // For AES: append "sAlT" marker
    if is_aes {
        hasher.update(b"sAlT");
    }

    let hash = hasher.finalize();
    let n = (key_length + 5).min(16);
    hash[..n].to_vec()
}

/// AES-128-CBC decrypt. First 16 bytes of data are IV.
fn aes128_cbc_decrypt_with_iv(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 16 {
        return Err(Error::Format("AES-128 data too short for IV".into()));
    }

    let iv = &data[..16];
    let ciphertext = &data[16..];

    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }

    // AES-CBC requires block-aligned input
    let block_aligned_len = (ciphertext.len() / 16) * 16;
    if block_aligned_len == 0 {
        return Ok(Vec::new());
    }

    let mut buf = ciphertext[..block_aligned_len].to_vec();

    let key_arr: [u8; 16] = key[..16.min(key.len())]
        .try_into()
        .map_err(|_| Error::Format("invalid AES-128 key length".into()))?;
    let iv_arr: [u8; 16] = iv
        .try_into()
        .map_err(|_| Error::Format("invalid IV length".into()))?;

    let decryptor = Aes128CbcDec::new(&key_arr.into(), &iv_arr.into());
    let decrypted = decryptor
        .decrypt_padded::<NoPadding>(&mut buf)
        .map_err(|_| Error::Format("AES-128-CBC decryption failed".into()))?;

    let mut result = decrypted.to_vec();
    strip_pkcs7_padding(&mut result);
    Ok(result)
}

/// AES-256-CBC decrypt. First 16 bytes of data are IV.
fn aes256_cbc_decrypt_with_iv(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 16 {
        return Err(Error::Format("AES-256 data too short for IV".into()));
    }

    let iv = &data[..16];
    let ciphertext = &data[16..];

    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }

    let block_aligned_len = (ciphertext.len() / 16) * 16;
    if block_aligned_len == 0 {
        return Ok(Vec::new());
    }

    let mut buf = ciphertext[..block_aligned_len].to_vec();

    let key_arr: [u8; 32] = key[..32.min(key.len())]
        .try_into()
        .map_err(|_| Error::Format("invalid AES-256 key length".into()))?;
    let iv_arr: [u8; 16] = iv
        .try_into()
        .map_err(|_| Error::Format("invalid IV length".into()))?;

    let decryptor = Aes256CbcDec::new(&key_arr.into(), &iv_arr.into());
    let decrypted = decryptor
        .decrypt_padded::<NoPadding>(&mut buf)
        .map_err(|_| Error::Format("AES-256-CBC decryption failed".into()))?;

    let mut result = decrypted.to_vec();
    strip_pkcs7_padding(&mut result);
    Ok(result)
}

/// AES-256-CBC decrypt with explicit IV (not from data). For R6 key unwrap.
fn aes256_cbc_decrypt(key: &[u8], iv: &[u8; 16], data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return Some(Vec::new());
    }

    let block_aligned_len = (data.len() / 16) * 16;
    if block_aligned_len == 0 {
        return Some(Vec::new());
    }

    let mut buf = data[..block_aligned_len].to_vec();

    let key_arr: [u8; 32] = key[..32.min(key.len())].try_into().ok()?;

    let decryptor = Aes256CbcDec::new(&key_arr.into(), iv.into());
    let decrypted = decryptor.decrypt_padded::<NoPadding>(&mut buf).ok()?;

    Some(decrypted.to_vec())
}

// --- Utility functions ---

/// Pad or truncate a password to 32 bytes using the standard padding.
fn pad_password(password: &[u8]) -> [u8; 32] {
    let mut padded = [0u8; 32];
    let len = password.len().min(32);
    padded[..len].copy_from_slice(&password[..len]);
    padded[len..].copy_from_slice(&PDF_PASSWORD_PADDING[..32 - len]);
    padded
}

/// Truncate password to 127 bytes for R6.
fn truncate_password(password: &[u8]) -> &[u8] {
    &password[..password.len().min(127)]
}

/// The R5/R6 password hash (ISO 32000-2 §7.6.4.3.4).
///
/// R5 (Adobe's deprecated ExtensionLevel 3, Acrobat 9) is a single
/// SHA-256(password ‖ salt ‖ udata). R6 (the PDF 2.0 standard, and what every
/// modern Acrobat AES-256 file uses) feeds that digest through the
/// Algorithm 2.B hardening loop. The two are not interchangeable: applying the
/// R5 form to an R6 file fails to authenticate any correct password.
///
/// `udata` is empty for user auth, the first 48 bytes of /U for owner auth.
fn hash_r6(revision: u32, password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    hasher.update(udata);
    let initial = hasher.finalize().to_vec();
    if revision <= 5 {
        return initial;
    }
    hash_2b(password, &initial, udata)
}

/// Algorithm 2.B - the R6 hash hardening loop.
///
/// At least 64 rounds; each round AES-128-CBC-encrypts 64 repetitions of
/// (password ‖ K ‖ udata) with key/IV from K, picks SHA-256/384/512 by the
/// first 16 bytes of the ciphertext mod 3, and the loop continues past round
/// 64 while the ciphertext's last byte exceeds (rounds - 32). Specified in
/// ISO 32000-2 §7.6.4.3.4; verified against Acrobat-written files (the
/// corpus KAT).
fn hash_2b(password: &[u8], initial: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut k = initial.to_vec();
    let mut round: usize = 0;
    let mut e_last: u8 = 0;
    while round < 64 || (e_last as usize) > round - 32 {
        // K1 = (password ‖ K ‖ udata) × 64. Always a multiple of 16 bytes
        // (64 repetitions), so NoPadding CBC is exact.
        let mut chunk = Vec::with_capacity(password.len() + k.len() + udata.len());
        chunk.extend_from_slice(password);
        chunk.extend_from_slice(&k);
        chunk.extend_from_slice(udata);
        let mut k1 = Vec::with_capacity(chunk.len() * 64);
        for _ in 0..64 {
            k1.extend_from_slice(&chunk);
        }
        let e = aes128_cbc_encrypt(&k[..16], &k[16..32], &mut k1);
        // "first 16 bytes of E as a big-endian number mod 3": since
        // 256 ≡ 1 (mod 3), that equals the byte sum mod 3
        let m = e[..16].iter().map(|&b| b as u32).sum::<u32>() % 3;
        k = match m {
            0 => Sha256::digest(e).to_vec(),
            1 => Sha384::digest(e).to_vec(),
            _ => Sha512::digest(e).to_vec(),
        };
        e_last = *e.last().unwrap_or(&0);
        round += 1;
    }
    k.truncate(32);
    k
}

/// AES-128-CBC encrypt in place (no padding - Algorithm 2.B's K1 is always
/// block-aligned). Returns the encrypted slice of `buf`.
fn aes128_cbc_encrypt<'a>(key: &[u8], iv: &[u8], buf: &'a mut [u8]) -> &'a [u8] {
    let key_arr: [u8; 16] = key[..16].try_into().expect("caller passes 16-byte key");
    let iv_arr: [u8; 16] = iv[..16].try_into().expect("caller passes 16-byte IV");
    let len = buf.len();
    let encryptor = Aes128CbcEnc::new(&key_arr.into(), &iv_arr.into());
    encryptor
        .encrypt_padded::<NoPadding>(buf, len)
        .expect("block-aligned input")
}

/// Strip PKCS#7 padding from decrypted data.
fn strip_pkcs7_padding(data: &mut Vec<u8>) {
    if let Some(&last) = data.last() {
        let pad_len = last as usize;
        if pad_len > 0 && pad_len <= 16 && pad_len <= data.len() {
            if data[data.len() - pad_len..].iter().all(|&b| b == last) {
                data.truncate(data.len() - pad_len);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- RC4 tests ---

    #[test]
    fn av1_rc4_roundtrip() {
        let key = b"SecretKey";
        let plaintext = b"Hello, World!";
        let encrypted = rc4_crypt(key, plaintext);
        let decrypted = rc4_crypt(key, &encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn av1_rc4_known_vector() {
        // RC4 test vector: key="Key", plaintext="Plaintext"
        let key = b"Key";
        let plaintext = b"Plaintext";
        let encrypted = rc4_crypt(key, plaintext);
        // RC4("Key", "Plaintext") = BBF316E8D940AF0AD3
        assert_eq!(
            encrypted,
            &[0xBB, 0xF3, 0x16, 0xE8, 0xD9, 0x40, 0xAF, 0x0A, 0xD3]
        );
    }

    #[test]
    fn av1_rc4_empty() {
        let result = rc4_crypt(b"key", b"");
        assert!(result.is_empty());
    }

    // --- Password padding ---

    #[test]
    fn av1_pad_password_short() {
        let padded = pad_password(b"test");
        assert_eq!(&padded[..4], b"test");
        assert_eq!(&padded[4..], &PDF_PASSWORD_PADDING[..28]);
    }

    #[test]
    fn av1_pad_password_empty() {
        let padded = pad_password(b"");
        assert_eq!(padded, PDF_PASSWORD_PADDING);
    }

    #[test]
    fn av1_pad_password_full() {
        let pw = [b'A'; 32];
        let padded = pad_password(&pw);
        assert_eq!(padded, pw);
    }

    #[test]
    fn av1_pad_password_long() {
        let pw = [b'B'; 50];
        let padded = pad_password(&pw);
        assert_eq!(padded, [b'B'; 32]);
    }

    // --- PKCS#7 padding ---

    #[test]
    fn av1_strip_pkcs7() {
        let mut data = vec![1, 2, 3, 4, 4, 4, 4, 4];
        strip_pkcs7_padding(&mut data);
        assert_eq!(data, vec![1, 2, 3, 4]);
    }

    #[test]
    fn av1_strip_pkcs7_full_block() {
        let mut data = vec![16; 16];
        strip_pkcs7_padding(&mut data);
        assert!(data.is_empty());
    }

    #[test]
    fn av1_strip_pkcs7_no_padding() {
        let mut data = vec![1, 2, 3, 0]; // last byte 0 -> pad_len=0 -> no strip
        strip_pkcs7_padding(&mut data);
        assert_eq!(data, vec![1, 2, 3, 0]);
    }

    #[test]
    fn av1_strip_pkcs7_invalid() {
        let mut data = vec![1, 2, 3, 5]; // last=5 but only 4 bytes -> don't strip
        strip_pkcs7_padding(&mut data);
        assert_eq!(data, vec![1, 2, 3, 5]); // unchanged (pad_len > len would be caught)
    }

    // --- Object key derivation ---

    #[test]
    fn av1_derive_object_key_rc4() {
        let file_key = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        let obj_key = derive_object_key(&file_key, 10, 0, 5, false);
        // Should be MD5(file_key + LE(10,3bytes) + LE(0,2bytes)) truncated to 10 bytes
        assert_eq!(obj_key.len(), 10); // key_length(5) + 5 = 10
    }

    #[test]
    fn av1_derive_object_key_aes() {
        let file_key = vec![0x01; 16];
        let obj_key = derive_object_key(&file_key, 1, 0, 16, true);
        // AES adds "sAlT" marker -> different hash
        assert_eq!(obj_key.len(), 16); // min(16+5, 16) = 16
    }

    // --- AES-128 tests ---

    #[test]
    fn av1_aes128_short_data() {
        let key = [0u8; 16];
        let result = aes128_cbc_decrypt_with_iv(&key, &[0; 10]);
        assert!(result.is_err()); // too short for IV
    }

    #[test]
    fn av1_aes128_iv_only() {
        let key = [0u8; 16];
        let result = aes128_cbc_decrypt_with_iv(&key, &[0; 16]).unwrap();
        assert!(result.is_empty()); // just IV, no ciphertext
    }

    // --- AES-256 tests ---

    #[test]
    fn av1_aes256_short_data() {
        let key = [0u8; 32];
        let result = aes256_cbc_decrypt_with_iv(&key, &[0; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn av1_aes256_iv_only() {
        let key = [0u8; 32];
        let result = aes256_cbc_decrypt_with_iv(&key, &[0; 16]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn av1_aes256_cbc_decrypt_zeros() {
        let key = [0u8; 32];
        let iv = [0u8; 16];
        let result = aes256_cbc_decrypt(&key, &iv, &[]).unwrap();
        assert!(result.is_empty());
    }

    // --- SecurityHandler parsing ---

    #[test]
    fn av1_parse_encrypt_dict_v1() {
        let encrypt = PdfObject::Dict(vec![
            (b"Filter".to_vec(), PdfObject::Name(b"Standard".to_vec())),
            (b"V".to_vec(), PdfObject::Int(1)),
            (b"R".to_vec(), PdfObject::Int(2)),
            (b"Length".to_vec(), PdfObject::Int(40)),
            (b"O".to_vec(), PdfObject::String(vec![0; 32])),
            (b"U".to_vec(), PdfObject::String(vec![0; 32])),
            (b"P".to_vec(), PdfObject::Int(-3904)),
        ]);
        let trailer = PdfObject::Dict(vec![(
            b"ID".to_vec(),
            PdfObject::Array(vec![
                PdfObject::String(b"test-file-id-123".to_vec()),
                PdfObject::String(b"test-file-id-123".to_vec()),
            ]),
        )]);

        let handler = SecurityHandler::from_encrypt_dict(&encrypt, &trailer).unwrap();
        assert_eq!(handler.algorithm, EncryptionAlgorithm::Rc4_40);
        assert_eq!(handler.key_length, 5);
        assert_eq!(handler.revision, 2);
        assert_eq!(handler.permissions, -3904);
        assert_eq!(handler.file_id, b"test-file-id-123");
    }

    #[test]
    fn av1_parse_encrypt_dict_v4() {
        let encrypt = PdfObject::Dict(vec![
            (b"Filter".to_vec(), PdfObject::Name(b"Standard".to_vec())),
            (b"V".to_vec(), PdfObject::Int(4)),
            (b"R".to_vec(), PdfObject::Int(4)),
            (b"Length".to_vec(), PdfObject::Int(128)),
            (b"O".to_vec(), PdfObject::String(vec![1; 32])),
            (b"U".to_vec(), PdfObject::String(vec![2; 32])),
            (b"P".to_vec(), PdfObject::Int(-1028)),
        ]);
        let trailer = PdfObject::Dict(vec![]);

        let handler = SecurityHandler::from_encrypt_dict(&encrypt, &trailer).unwrap();
        assert_eq!(handler.algorithm, EncryptionAlgorithm::Aes128);
        assert_eq!(handler.key_length, 16);
        assert_eq!(handler.revision, 4);
    }

    #[test]
    fn av1_parse_encrypt_dict_v5() {
        let encrypt = PdfObject::Dict(vec![
            (b"Filter".to_vec(), PdfObject::Name(b"Standard".to_vec())),
            (b"V".to_vec(), PdfObject::Int(5)),
            (b"R".to_vec(), PdfObject::Int(6)),
            (b"Length".to_vec(), PdfObject::Int(256)),
            (b"O".to_vec(), PdfObject::String(vec![0xAA; 48])),
            (b"U".to_vec(), PdfObject::String(vec![0xBB; 48])),
            (b"OE".to_vec(), PdfObject::String(vec![0xCC; 32])),
            (b"UE".to_vec(), PdfObject::String(vec![0xDD; 32])),
            (b"Perms".to_vec(), PdfObject::String(vec![0xEE; 16])),
            (b"P".to_vec(), PdfObject::Int(-4)),
        ]);
        let trailer = PdfObject::Dict(vec![]);

        let handler = SecurityHandler::from_encrypt_dict(&encrypt, &trailer).unwrap();
        assert_eq!(handler.algorithm, EncryptionAlgorithm::Aes256);
        assert_eq!(handler.key_length, 32);
        assert_eq!(handler.revision, 6);
        assert!(handler.oe.is_some());
        assert!(handler.ue.is_some());
        assert!(handler.perms.is_some());
    }

    // --- AV2: User password authentication ---

    #[test]
    fn av2_empty_password_v1() {
        // Build a SecurityHandler where empty password should authenticate
        // We pre-compute the expected values for V1/R2 with empty password
        let file_id = b"0123456789abcdef".to_vec();
        let permissions: i32 = -3904;

        // Compute encryption key for empty password
        let padded = pad_password(b"");
        let mut hasher = Md5::new();
        hasher.update(padded);
        hasher.update(&[0u8; 32]); // /O = zeros
        hasher.update((permissions as u32).to_le_bytes());
        hasher.update(&file_id);
        let key = hasher.finalize()[..5].to_vec();

        // Compute /U = RC4(key, padding)
        let user_hash = rc4_crypt(&key, &PDF_PASSWORD_PADDING);

        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash: vec![0; 32],
            user_hash,
            permissions,
            file_id,
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(handler.authenticate_user(b""));
        assert!(handler.is_authenticated());
    }

    #[test]
    fn av2_wrong_password() {
        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash: vec![0; 32],
            user_hash: vec![0xFF; 32], // Won't match
            permissions: 0,
            file_id: vec![0; 16],
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(!handler.authenticate_user(b"wrong"));
        assert!(!handler.is_authenticated());
    }

    // --- AV3: Owner password authentication ---

    #[test]
    fn av3_owner_password_v1() {
        // Build a scenario: owner password = "owner", user password = empty
        let file_id = b"test-id-12345678".to_vec();
        let permissions: i32 = -3904;

        // Compute /O from owner password "owner"
        let owner_padded = pad_password(b"owner");
        let owner_key_hash = Md5::digest(&owner_padded);
        let owner_key = owner_key_hash[..5].to_vec();

        // /O = RC4(owner_key, pad(user_password=""))
        let user_padded = pad_password(b"");
        let owner_hash = rc4_crypt(&owner_key, &user_padded);

        // Compute encryption key for user password ""
        let mut hasher = Md5::new();
        hasher.update(user_padded);
        hasher.update(&owner_hash);
        hasher.update((permissions as u32).to_le_bytes());
        hasher.update(&file_id);
        let key = hasher.finalize()[..5].to_vec();

        let user_hash = rc4_crypt(&key, &PDF_PASSWORD_PADDING);

        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash,
            user_hash,
            permissions,
            file_id,
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        // Owner password should work
        assert!(handler.authenticate_owner(b"owner"));
        assert!(handler.is_authenticated());
    }

    #[test]
    fn av3_wrong_owner_password() {
        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash: vec![0xAA; 32],
            user_hash: vec![0xBB; 32],
            permissions: 0,
            file_id: vec![0; 16],
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(!handler.authenticate_owner(b"wrong"));
    }

    // --- AV4: Permission checking ---

    #[test]
    fn av4_permissions_all_set() {
        let perms = PdfPermissions { raw: -1 }; // All bits set
        assert!(perms.can_print());
        assert!(perms.can_modify());
        assert!(perms.can_copy());
        assert!(perms.can_annotate());
        assert!(perms.can_fill_forms());
        assert!(perms.can_extract_accessibility());
        assert!(perms.can_assemble());
        assert!(perms.can_print_high_quality());
    }

    #[test]
    fn av4_permissions_none_set() {
        let perms = PdfPermissions { raw: 0 };
        assert!(!perms.can_print());
        assert!(!perms.can_modify());
        assert!(!perms.can_copy());
        assert!(!perms.can_annotate());
        assert!(!perms.can_fill_forms());
        assert!(!perms.can_extract_accessibility());
        assert!(!perms.can_assemble());
        assert!(!perms.can_print_high_quality());
    }

    #[test]
    fn av4_permissions_common_restricted() {
        // "No modify, no copy" - bits 3,6,9,10,11,12 set + bits 13-32 set
        // = 4 + 32 + 256 + 512 + 1024 + 2048 + 0xFFFFF000 = 0xFFFFFF24 = -220
        let perms = PdfPermissions { raw: -220 };
        assert!(perms.can_print()); // bit 3
        assert!(!perms.can_modify()); // bit 4 clear
        assert!(!perms.can_copy()); // bit 5 clear
        assert!(perms.can_annotate()); // bit 6
        assert!(perms.can_fill_forms()); // bit 9
        assert!(perms.can_extract_accessibility()); // bit 10
        assert!(perms.can_assemble()); // bit 11
        assert!(perms.can_print_high_quality()); // bit 12
    }

    #[test]
    fn av4_permissions_print_only() {
        let perms = PdfPermissions { raw: 4 }; // Only bit 3
        assert!(perms.can_print());
        assert!(!perms.can_modify());
        assert!(!perms.can_copy());
    }

    // --- R2 encryption key + decryption end-to-end ---

    #[test]
    fn av1_decrypt_stream_rc4() {
        let file_id = b"abcdefghijklmnop".to_vec();
        let permissions: i32 = -4;

        // Build handler with known key
        let padded = pad_password(b"");
        let mut hasher = Md5::new();
        hasher.update(padded);
        hasher.update(&[0u8; 32]); // /O
        hasher.update((permissions as u32).to_le_bytes());
        hasher.update(&file_id);
        let file_key = hasher.finalize()[..5].to_vec();
        let user_hash = rc4_crypt(&file_key, &PDF_PASSWORD_PADDING);

        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash: vec![0; 32],
            user_hash,
            permissions,
            file_id,
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(handler.authenticate_user(b""));

        // Encrypt "Hello" with object 1 gen 0
        let obj_key = derive_object_key(&file_key, 1, 0, 5, false);
        let encrypted = rc4_crypt(&obj_key, b"Hello");

        // Decrypt should recover original
        let decrypted = handler.decrypt_stream(1, 0, &encrypted).unwrap();
        assert_eq!(decrypted, b"Hello");
    }

    #[test]
    fn av1_decrypt_not_authenticated() {
        let handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash: vec![0; 32],
            user_hash: vec![0; 32],
            permissions: 0,
            file_id: vec![0; 16],
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(handler.decrypt_stream(1, 0, b"data").is_err());
    }

    // --- try_empty_password ---

    #[test]
    fn av2_try_empty_password() {
        let file_id = b"empty-password!!".to_vec();
        let permissions: i32 = -4;

        let padded = pad_password(b"");
        let mut hasher = Md5::new();
        hasher.update(padded);
        hasher.update(&[0u8; 32]);
        hasher.update((permissions as u32).to_le_bytes());
        hasher.update(&file_id);
        let file_key = hasher.finalize()[..5].to_vec();
        let user_hash = rc4_crypt(&file_key, &PDF_PASSWORD_PADDING);

        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_40,
            key_length: 5,
            revision: 2,
            owner_hash: vec![0; 32],
            user_hash,
            permissions,
            file_id,
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(handler.try_empty_password());
        assert!(handler.is_authenticated());
    }

    // --- R6 user authentication ---

    #[test]
    fn av2_r6_wrong_password() {
        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Aes256,
            key_length: 32,
            revision: 6,
            owner_hash: vec![0; 48],
            user_hash: vec![0xFF; 48], // Won't match any SHA-256
            permissions: 0,
            file_id: vec![],
            encrypt_metadata: true,
            oe: None,
            ue: Some(vec![0; 32]),
            perms: None,
            encryption_key: None,
        };

        assert!(!handler.authenticate_user(b"wrong"));
    }

    // --- Debug: verify MD5 matches Python ---

    #[test]
    fn av2_key_derivation_orientation_pdf() {
        // Test key derivation against orientation.pdf (V=2, R=3, 128-bit RC4)
        let o_hash: Vec<u8> = vec![
            0x9a, 0x96, 0xe5, 0x24, 0x6f, 0x68, 0x89, 0x17, 0x95, 0x47, 0x37, 0x64, 0xc9, 0xcf,
            0x4f, 0xbc, 0x9e, 0xbf, 0xd7, 0x99, 0x18, 0x5f, 0x59, 0x3e, 0x8d, 0x34, 0x72, 0x7e,
            0x26, 0x9e, 0x07, 0x15,
        ];
        let u_hash: Vec<u8> = vec![
            0xe4, 0xbb, 0x26, 0x9f, 0x71, 0xac, 0xc2, 0xcd, 0x68, 0xb0, 0x47, 0xe1, 0x35, 0xb7,
            0x4b, 0x70, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        let file_id: Vec<u8> = vec![
            0x12, 0xB0, 0x59, 0x84, 0xC1, 0xB7, 0xBB, 0xA3, 0xC1, 0x09, 0x09, 0xB1, 0x78, 0x09,
            0x4A, 0xC4,
        ];

        let mut handler = SecurityHandler {
            algorithm: EncryptionAlgorithm::Rc4_128,
            key_length: 16,
            revision: 3,
            owner_hash: o_hash,
            user_hash: u_hash,
            permissions: -2364,
            file_id,
            encrypt_metadata: true,
            oe: None,
            ue: None,
            perms: None,
            encryption_key: None,
        };

        assert!(
            handler.authenticate_user(b""),
            "empty password should authenticate"
        );
        assert!(handler.is_authenticated());

        // Verify the derived key matches expected
        let expected_key: Vec<u8> = vec![
            0xd5, 0x07, 0x22, 0x9c, 0x1d, 0x78, 0xdf, 0xa4, 0xb5, 0xc7, 0x46, 0x28, 0x89, 0x0c,
            0x0a, 0xc5,
        ];
        assert_eq!(handler.encryption_key.as_ref().unwrap(), &expected_key);
    }

    // --- Truncate password ---

    #[test]
    fn av1_truncate_password() {
        let short = b"hello";
        assert_eq!(truncate_password(short), b"hello");

        let long = vec![b'A'; 200];
        assert_eq!(truncate_password(&long).len(), 127);
    }
}
