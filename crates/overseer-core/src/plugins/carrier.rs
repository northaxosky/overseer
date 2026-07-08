//! Emitting a record-free Fallout 4 carrier ESL by hand

/// TES4 record flags: master (0x1) plus light/ESL (0x200)
const CARRIER_FLAGS: u32 = 0x0000_0201;

/// Author stamped into the carrier's CNAM subrecord
const CARRIER_AUTHOR: &str = "Overseer";

/// Append one subrecord: 4-byte type, u16 data size, then the data
fn push_subrecord(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(kind);
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
}

/// Build a minimal record-free Fallout 4 carrier ESL so the game auto-loads merged CC
pub fn carrier_esl() -> Vec<u8> {
    let mut payload = Vec::new();

    // HEDR: f32 version 1.0 + 2 u32
    let mut hedr = Vec::with_capacity(12);
    hedr.extend_from_slice(&1.0f32.to_le_bytes());
    hedr.extend_from_slice(&0u32.to_le_bytes());
    hedr.extend_from_slice(&0u32.to_le_bytes());
    push_subrecord(&mut payload, b"HEDR", &hedr);

    // CNAM: null-terminated author string
    let mut author = CARRIER_AUTHOR.as_bytes().to_vec();
    author.push(0);
    push_subrecord(&mut payload, b"CNAM", &author);

    // INTV: u32 tagified-string count
    push_subrecord(&mut payload, b"INTV", &0u32.to_le_bytes());

    // 24-byte TES4 record header
    let mut out = Vec::with_capacity(24 + payload.len());
    out.extend_from_slice(b"TES4");
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&CARRIER_FLAGS.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

#[cfg(test)]
#[path = "tests/carrier.rs"]
mod tests;
