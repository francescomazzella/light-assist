//! The devices the sharer trusts to connect while nobody is at this computer.
//!
//! A device is a helper's browser that was given a secret during an ordinary session, with the sharer
//! there to say so. Later it proves that it holds the secret without sending it: the agent makes up a
//! number, and the browser answers with a keyed hash of it (HMAC-SHA-256). Someone listening on the
//! network learns nothing they can use again.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{util::now_ms, words};

/// More than anyone needs; a forgotten list should not grow without end.
const MAX_DEVICES: usize = 16;
const MAX_NAME_CHARS: usize = 40;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Device {
    id: String,
    /// What the sharer called it.
    name: String,
    /// 32 bytes, as hexadecimal. Never leaves this computer again once the device has it.
    secret: String,
    added_at: u64,
    last_used: Option<u64>,
}

/// What the console shows of a device: everything but its secret.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub added_at: u64,
    pub last_used: Option<u64>,
}

/// What a device is handed when it is trusted.
#[derive(Clone, Debug)]
pub struct Enrollment {
    pub id: String,
    pub secret: String,
    pub name: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Devices {
    /// The random part of this computer's help address for as long as unattended access is on. It
    /// stays the same, because a browser keeps a device's secret under the address it was given at.
    name: Option<String>,
    devices: Vec<Device>,
}

fn file() -> Option<PathBuf> {
    Some(crate::settings::data_folder()?.join("devices.json"))
}

/// `bytes` random bytes as hexadecimal.
pub fn random_hex(bytes: usize) -> String {
    let mut random = vec![0u8; bytes];
    getrandom::fill(&mut random).expect("OS randomness is unavailable");
    hex(&random)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok()).collect()
}

impl Devices {
    pub fn load() -> Self {
        file().and_then(|path| std::fs::read(path).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    fn save(&self) {
        let Some(path) = file() else { return };
        let written = path.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| {
            let contents = serde_json::to_vec_pretty(self).unwrap_or_default();
            // The secrets are for this user only.
            #[cfg(unix)]
            {
                use std::{io::Write, os::unix::fs::OpenOptionsExt};
                std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)?.write_all(&contents)
            }
            #[cfg(not(unix))]
            std::fs::write(&path, contents)
        });
        if let Err(error) = written {
            eprintln!("The trusted devices could not be saved: {error}");
        }
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    pub fn list(&self) -> Vec<DeviceInfo> {
        self.devices
            .iter()
            .map(|device| DeviceInfo { id: device.id.clone(), name: device.name.clone(), added_at: device.added_at, last_used: device.last_used })
            .collect()
    }

    /// This computer's lasting help address, such as `assist-maple-river.local`, chosen the first time it is needed.
    pub fn address_name(&mut self) -> String {
        if self.name.is_none() {
            self.name = Some(words::lasting());
            self.save();
        }
        format!("assist-{}.local", self.name.as_deref().unwrap_or_default())
    }

    /// Trusts a new device, and returns what it has to be given.
    pub fn add(&mut self, name: &str) -> Option<Enrollment> {
        if self.devices.len() >= MAX_DEVICES {
            return None;
        }
        let name: String = name.trim().chars().filter(|character| !character.is_control()).take(MAX_NAME_CHARS).collect();
        let name = if name.is_empty() { "A helper's browser".to_owned() } else { name };
        let enrollment = Enrollment { id: random_hex(8), secret: random_hex(32), name: name.clone() };
        self.devices.push(Device { id: enrollment.id.clone(), name, secret: enrollment.secret.clone(), added_at: now_ms(), last_used: None });
        self.save();
        Some(enrollment)
    }

    /// Stops trusting a device, and reports whether there was one.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.devices.len();
        self.devices.retain(|device| device.id != id);
        let removed = self.devices.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    /// The name of the device `id`, if `proof` shows that it holds its secret: the keyed hash of the
    /// `challenge` it was just sent.
    pub fn verify(&mut self, id: &str, challenge: &str, proof: &str) -> Option<String> {
        let device = self.devices.iter_mut().find(|device| device.id == id)?;
        let expected = hmac_sha256(&unhex(&device.secret)?, challenge.as_bytes());
        let given = unhex(proof)?;
        // Compared without stopping at the first difference, so that timing says nothing about it.
        let same = given.len() == expected.len() && given.iter().zip(expected).fold(0, |different, (given, expected)| different | (given ^ expected)) == 0;
        if !same {
            return None;
        }
        device.last_used = Some(now_ms());
        let name = device.name.clone();
        self.save();
        Some(name)
    }
}

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74,
    0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d,
    0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e,
    0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256, written out here so that the agent needs no library for one hash.
pub fn sha256(message: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut padded = message.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&(message.len() as u64 * 8).to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in block.chunks_exact(4).enumerate() {
            w[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7) ^ w[index - 15].rotate_right(18) ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17) ^ w[index - 2].rotate_right(19) ^ (w[index - 2] >> 10);
            w[index] = w[index - 16].wrapping_add(s0).wrapping_add(w[index - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let t1 = h.wrapping_add(s1).wrapping_add((e & f) ^ (!e & g)).wrapping_add(K[index]).wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let t2 = s0.wrapping_add((a & b) ^ (a & c) ^ (b & c));
            (h, g, f, e, d, c, b, a) = (g, f, e, d.wrapping_add(t1), c, b, a, t1.wrapping_add(t2));
        }
        for (value, add) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *value = value.wrapping_add(add);
        }
    }
    let mut out = [0u8; 32];
    for (bytes, word) in out.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    out
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&sha256(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let inner: Vec<u8> = block.iter().map(|byte| byte ^ 0x36).chain(message.iter().copied()).collect();
    let outer: Vec<u8> = block.iter().map(|byte| byte ^ 0x5c).chain(sha256(&inner)).collect();
    sha256(&outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_values() {
        assert_eq!(hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(hex(&sha256(&[b'a'; 200])), "c2a908d98f5df987ade41b5fce213067efbcc21ef2240212a41e54b5e7c28ae5");
        // RFC 4231, test case 2.
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn only_the_holder_of_the_secret_is_let_in() {
        let mut devices = Devices::default();
        devices.devices.push(Device { id: "ab".into(), name: "Laptop".into(), secret: hex(&[7; 32]), added_at: 0, last_used: None });
        let proof = hex(&hmac_sha256(&[7; 32], b"challenge"));
        // Checked without saving anything: these never reach `verify`'s end.
        assert_eq!(devices.verify("ab", "another challenge", &proof), None);
        assert_eq!(devices.verify("cd", "challenge", &proof), None);
        assert_eq!(devices.verify("ab", "challenge", "not hex"), None);
        assert_eq!(devices.verify("ab", "challenge", &proof[..62]), None);
    }
}
