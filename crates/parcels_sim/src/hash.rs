/// FNV-1a, 64-bit. Simple, portable, stable across platforms and Rust versions
/// (unlike `std::hash::DefaultHasher`, which is explicitly unspecified).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
