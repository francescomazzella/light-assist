//! The pairing code, worked out by both ends instead of handed from one to the other.
//!
//! The code used to be a random number the agent sent to the helper page. Whatever carried it could
//! have sat between the two, speaking to each as the other, and the code would not have shown it.
//! Now the code is a digest of the two ends' encryption certificates. Someone in between has to
//! present a certificate of their own to each side, so the two sides arrive at different codes, and
//! the one the helper reads out is not the one the sharer's console accepts.
//!
//! Six digits would be few enough to search for a pair of certificates that happen to agree, so each
//! side also puts in a random number, and the helper page commits to its own before it is told the
//! agent's: by the time anyone can work a code out, nothing that goes into it can still be chosen.
//!
//!   agent → page   hello     { protocol }
//!   page → agent   commit    { protocol, commit }        commit = SHA-256(page's number | page's fingerprint)
//!   agent → page   pairing   { nonce, fingerprint }      the agent's number and fingerprint
//!   page → agent   reveal    { nonce, fingerprint }      the page's, which must match its commitment
//!
//! The connection that follows is only accepted, by each end, from the certificate named here.

use crate::devices::{hex, sha256};

/// Which version of the signaling messages this app speaks. The helper page says so too, and each
/// tells its user when the other is older than it can work with.
pub const PROTOCOL: u32 = 2;

/// What the helper page binds itself to before it learns the agent's number.
pub fn commitment(nonce: &str, fingerprint: &str) -> String {
    hex(&sha256(format!("{nonce}|{fingerprint}").as_bytes()))
}

/// The six digits both ends arrive at, if they are talking to each other and to nobody else.
pub fn code(helper_fingerprint: &str, agent_fingerprint: &str, helper_nonce: &str, agent_nonce: &str) -> String {
    let digest = sha256(format!("{helper_fingerprint}|{agent_fingerprint}|{helper_nonce}|{agent_nonce}").as_bytes());
    format!("{:06}", u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) % 1_000_000)
}

/// What a trusted device signs with its secret: the agent's challenge and both certificates, so that
/// its proof is good for this connection and no other.
pub fn device_challenge(nonce: &str, agent_fingerprint: &str, helper_fingerprint: &str) -> String {
    format!("{nonce}|{agent_fingerprint}|{helper_fingerprint}")
}

/// A certificate's fingerprint as SDP writes it: `AB:CD:…`. Told apart from anything else that might
/// be sent in its place.
pub fn is_fingerprint(text: &str) -> bool {
    text.len() == 95 && text.split(':').all(|pair| pair.len() == 2 && pair.bytes().all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)))
}

/// The fingerprint an SDP offer or answer names for its end.
pub fn fingerprint_in(sdp: &str) -> Option<String> {
    sdp.lines().find_map(|line| line.trim().strip_prefix("a=fingerprint:sha-256 ")).map(|fingerprint| fingerprint.trim().to_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_is_six_digits_and_changes_with_everything_that_goes_into_it() {
        let (helper, agent) = ("AA:".repeat(31) + "AA", "BB:".repeat(31) + "BB");
        let code_ = code(&helper, &agent, "01", "02");
        assert!(code_.len() == 6 && code_.bytes().all(|byte| byte.is_ascii_digit()));
        // The helper page works the same code out for itself (apps/ui/src/pairing.ts).
        assert_eq!(code_, "538349");
        let others = [code(&agent, &agent, "01", "02"), code(&helper, &helper, "01", "02"), code(&helper, &agent, "03", "02"), code(&helper, &agent, "01", "03")];
        assert!(others.iter().all(|other| *other != code_));
        assert!(is_fingerprint(&helper) && !is_fingerprint("AA:BB") && !is_fingerprint(&helper.to_lowercase()));
        assert_eq!(fingerprint_in(&format!("v=0\r\na=fingerprint:sha-256 {}\r\n", helper.to_lowercase())), Some(helper));
    }
}
