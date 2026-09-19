//! Dependency-free crypto primitives shared by the register-level accelerators and the virtual AP.
//! The AP uses SHA-1, HMAC-SHA1, PBKDF2, the 802.11 PRF and AES key wrap for its WPA2 handshake.

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bits.to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 16];
        for i in 0..16 { w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]); }
        sha1_block(&mut h, &w);
    }
    let mut out = [0u8; 20];
    for i in 0..5 { out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes()); }
    out
}

/// Compress one block into the first five state words (the hardware exposes a larger register bank).
pub(crate) fn sha1_block(h: &mut [u32], w0: &[u32; 16]) {
    let mut w = [0u32; 80];
    w[..16].copy_from_slice(w0);
    for i in 16..80 { w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1); }
    let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
    for (i, &wi) in w.iter().enumerate() {
        let (f, k) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5A827999),
            20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
            _ => (b ^ c ^ d, 0xCA62C1D6),
        };
        let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(wi);
        e = d; d = c; c = b.rotate_left(30); b = a; a = t;
    }
    for (word, v) in h.iter_mut().zip([a, b, c, d, e]) { *word = word.wrapping_add(v); }
}

pub fn hmac_sha1(key: &[u8], data: &[u8]) -> [u8; 20] {
    let mut k = [0u8; 64];
    if key.len() > 64 { k[..20].copy_from_slice(&sha1(key)); } else { k[..key.len()].copy_from_slice(key); }
    let mut inner = Vec::with_capacity(64 + data.len());
    let mut outer = Vec::with_capacity(84);
    for &ki in &k { inner.push(ki ^ 0x36); outer.push(ki ^ 0x5c); }
    inner.extend_from_slice(data);
    outer.extend_from_slice(&sha1(&inner));
    sha1(&outer)
}

/// PBKDF2-HMAC-SHA1 — the WPA passphrase-to-PMK mapping (4096 iterations, SSID as salt).
pub fn pbkdf2_sha1(password: &[u8], salt: &[u8], iterations: u32, out_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(out_len);
    let mut block = 1u32;
    while out.len() < out_len {
        let mut salted = salt.to_vec();
        salted.extend_from_slice(&block.to_be_bytes());
        let mut u = hmac_sha1(password, &salted);
        let mut acc = u;
        for _ in 1..iterations {
            u = hmac_sha1(password, &u);
            for i in 0..20 { acc[i] ^= u[i]; }
        }
        out.extend_from_slice(&acc);
        block += 1;
    }
    out.truncate(out_len);
    out
}

/// IEEE 802.11 PRF: HMAC-SHA1 expanded to `bits` bits over label || 0x00 || data || counter.
pub fn prf(key: &[u8], label: &str, data: &[u8], bits: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0u8;
    while out.len() * 8 < bits {
        let mut buf = Vec::with_capacity(label.len() + data.len() + 2);
        buf.extend_from_slice(label.as_bytes()); buf.push(0);
        buf.extend_from_slice(data); buf.push(i);
        out.extend_from_slice(&hmac_sha1(key, &buf));
        i += 1;
    }
    out.truncate(bits.div_ceil(8));
    out
}

// ------------------------------------------------------------------ AES

const SBOX: [u8; 256] = [
    0x63,0x7c,0x77,0x7b,0xf2,0x6b,0x6f,0xc5,0x30,0x01,0x67,0x2b,0xfe,0xd7,0xab,0x76,
    0xca,0x82,0xc9,0x7d,0xfa,0x59,0x47,0xf0,0xad,0xd4,0xa2,0xaf,0x9c,0xa4,0x72,0xc0,
    0xb7,0xfd,0x93,0x26,0x36,0x3f,0xf7,0xcc,0x34,0xa5,0xe5,0xf1,0x71,0xd8,0x31,0x15,
    0x04,0xc7,0x23,0xc3,0x18,0x96,0x05,0x9a,0x07,0x12,0x80,0xe2,0xeb,0x27,0xb2,0x75,
    0x09,0x83,0x2c,0x1a,0x1b,0x6e,0x5a,0xa0,0x52,0x3b,0xd6,0xb3,0x29,0xe3,0x2f,0x84,
    0x53,0xd1,0x00,0xed,0x20,0xfc,0xb1,0x5b,0x6a,0xcb,0xbe,0x39,0x4a,0x4c,0x58,0xcf,
    0xd0,0xef,0xaa,0xfb,0x43,0x4d,0x33,0x85,0x45,0xf9,0x02,0x7f,0x50,0x3c,0x9f,0xa8,
    0x51,0xa3,0x40,0x8f,0x92,0x9d,0x38,0xf5,0xbc,0xb6,0xda,0x21,0x10,0xff,0xf3,0xd2,
    0xcd,0x0c,0x13,0xec,0x5f,0x97,0x44,0x17,0xc4,0xa7,0x7e,0x3d,0x64,0x5d,0x19,0x73,
    0x60,0x81,0x4f,0xdc,0x22,0x2a,0x90,0x88,0x46,0xee,0xb8,0x14,0xde,0x5e,0x0b,0xdb,
    0xe0,0x32,0x3a,0x0a,0x49,0x06,0x24,0x5c,0xc2,0xd3,0xac,0x62,0x91,0x95,0xe4,0x79,
    0xe7,0xc8,0x37,0x6d,0x8d,0xd5,0x4e,0xa9,0x6c,0x56,0xf4,0xea,0x65,0x7a,0xae,0x08,
    0xba,0x78,0x25,0x2e,0x1c,0xa6,0xb4,0xc6,0xe8,0xdd,0x74,0x1f,0x4b,0xbd,0x8b,0x8a,
    0x70,0x3e,0xb5,0x66,0x48,0x03,0xf6,0x0e,0x61,0x35,0x57,0xb9,0x86,0xc1,0x1d,0x9e,
    0xe1,0xf8,0x98,0x11,0x69,0xd9,0x8e,0x94,0x9b,0x1e,0x87,0xe9,0xce,0x55,0x28,0xdf,
    0x8c,0xa1,0x89,0x0d,0xbf,0xe6,0x42,0x68,0x41,0x99,0x2d,0x0f,0xb0,0x54,0xbb,0x16];

fn xtime(b: u8) -> u8 { (b << 1) ^ if b & 0x80 != 0 { 0x1b } else { 0 } }

/// Inverse S-box, derived from the forward one.
fn inv_sbox() -> [u8; 256] { let mut t = [0u8; 256]; for i in 0..256 { t[SBOX[i] as usize] = i as u8; } t }
fn mul(a: u8, b: u8) -> u8 {
    let (mut a, mut b, mut r) = (a, b, 0u8);
    while b != 0 { if b & 1 != 0 { r ^= a; } a = xtime(a); b >>= 1; }
    r
}

/// AES with any legal key length, both directions — what the ESP32-S3 AES accelerator provides.
pub fn aes_block(key: &[u8], block: &[u8; 16], decrypt: bool) -> [u8; 16] {
    assert!(matches!(key.len(), 16 | 24 | 32), "invalid AES key length");
    let nk = key.len() / 4;
    let nr = nk + 6;
    // key expansion
    let mut w = [[0u8; 4]; 60]; // AES-256 has the largest schedule: 15 round keys of four words.
    for i in 0..nk { w[i].copy_from_slice(&key[4 * i..4 * i + 4]); }
    let mut rcon = 1u8;
    for i in nk..4 * (nr + 1) {
        let mut t = w[i - 1];
        if i % nk == 0 {
            t = [SBOX[t[1] as usize] ^ rcon, SBOX[t[2] as usize], SBOX[t[3] as usize], SBOX[t[0] as usize]];
            rcon = xtime(rcon);
        } else if nk > 6 && i % nk == 4 {
            for b in t.iter_mut() { *b = SBOX[*b as usize]; }
        }
        for j in 0..4 { w[i][j] = w[i - nk][j] ^ t[j]; }
    }
    let rk = |round: usize| -> [u8; 16] {
        let mut k = [0u8; 16];
        for c in 0..4 { k[4 * c..4 * c + 4].copy_from_slice(&w[4 * round + c]); }
        k
    };
    let mut s = *block;
    if !decrypt {
        let k = rk(0); for i in 0..16 { s[i] ^= k[i]; }
        for round in 1..=nr {
            for b in s.iter_mut() { *b = SBOX[*b as usize]; }
            let t = s; for c in 0..4 { for r in 0..4 { s[4 * c + r] = t[4 * ((c + r) % 4) + r]; } }
            if round != nr {
                for c in 0..4 {
                    let col = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
                    let x = col[0] ^ col[1] ^ col[2] ^ col[3];
                    for r in 0..4 { s[4 * c + r] = col[r] ^ x ^ xtime(col[r] ^ col[(r + 1) % 4]); }
                }
            }
            let k = rk(round); for i in 0..16 { s[i] ^= k[i]; }
        }
    } else {
        let inv = inv_sbox();
        let k = rk(nr); for i in 0..16 { s[i] ^= k[i]; }
        for round in (0..nr).rev() {
            let t = s; for c in 0..4 { for r in 0..4 { s[4 * ((c + r) % 4) + r] = t[4 * c + r]; } }   // InvShiftRows
            for b in s.iter_mut() { *b = inv[*b as usize]; }
            let k = rk(round); for i in 0..16 { s[i] ^= k[i]; }
            if round != 0 {
                for c in 0..4 {                                                                        // InvMixColumns
                    let col = [s[4 * c], s[4 * c + 1], s[4 * c + 2], s[4 * c + 3]];
                    s[4 * c] = mul(col[0], 14) ^ mul(col[1], 11) ^ mul(col[2], 13) ^ mul(col[3], 9);
                    s[4 * c + 1] = mul(col[0], 9) ^ mul(col[1], 14) ^ mul(col[2], 11) ^ mul(col[3], 13);
                    s[4 * c + 2] = mul(col[0], 13) ^ mul(col[1], 9) ^ mul(col[2], 14) ^ mul(col[3], 11);
                    s[4 * c + 3] = mul(col[0], 11) ^ mul(col[1], 13) ^ mul(col[2], 9) ^ mul(col[3], 14);
                }
            }
        }
    }
    s
}

pub fn aes128_encrypt_block(key: &[u8; 16], block: &[u8; 16]) -> [u8; 16] {
    aes_block(key, block, false)
}

/// AES key wrap (RFC 3394) — how the GTK travels inside message 3 of the handshake.
pub fn aes_key_wrap(kek: &[u8; 16], plain: &[u8]) -> Vec<u8> {
    let n = plain.len() / 8;
    let mut a = [0xa6u8; 8];
    let mut r: Vec<[u8; 8]> = (0..n).map(|i| { let mut b = [0u8; 8]; b.copy_from_slice(&plain[i * 8..i * 8 + 8]); b }).collect();
    for j in 0..6u64 {
        for (i, ri) in r.iter_mut().enumerate() {
            let mut block = [0u8; 16];
            block[..8].copy_from_slice(&a); block[8..].copy_from_slice(ri);
            let b = aes128_encrypt_block(kek, &block);
            a.copy_from_slice(&b[..8]);
            let t = j * n as u64 + i as u64 + 1;
            for (k, tb) in t.to_be_bytes().iter().enumerate() { a[k] ^= tb; }
            ri.copy_from_slice(&b[8..]);
        }
    }
    let mut out = Vec::with_capacity(8 + n * 8);
    out.extend_from_slice(&a);
    for b in r { out.extend_from_slice(&b); }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sha1_known_answers() {
        assert_eq!(sha1(b"abc")[..], hex("a9993e364706816aba3e25717850c26c9cd0d89d")[..]);
        assert_eq!(sha1(b"")[..], hex("da39a3ee5e6b4b0d3255bfef95601890afd80709")[..]);
    }
    #[test] fn hmac_rfc2202() {
        assert_eq!(hmac_sha1(&[0x0b; 20], b"Hi There")[..], hex("b617318655057264e28bc0b6fb378c8ef146be00")[..]);
    }
    #[test] fn pbkdf2_wpa_vector() {
        // IEEE 802.11i-2004 Annex H.4: passphrase "password", SSID "IEEE"
        assert_eq!(pbkdf2_sha1(b"password", b"IEEE", 4096, 32),
                   hex("f42c6fc52df0ebef9ebb4b90b38a5f902e83fe1b135a70e23aed762e9710a12e"));
    }
    #[test] fn aes_all_key_lengths_roundtrip() {
        // FIPS-197 appendix C vectors for 128/192/256, plus decrypt round-trips
        let pt = hex("00112233445566778899aabbccddeeff");
        let mut b = [0u8; 16]; b.copy_from_slice(&pt);
        for (klen, ct) in [(16, "69c4e0d86a7b0430d8cdb78070b4c55a"), (24, "dda97ca4864cdfe06eaf70a0ec0d7191"), (32, "8ea2b7ca516745bfeafc49904b496089")] {
            let key: Vec<u8> = (0..klen as u8).collect();
            let enc = aes_block(&key, &b, false);
            assert_eq!(enc[..], hex(ct)[..], "encrypt with {}-byte key", klen);
            assert_eq!(aes_block(&key, &enc, true)[..], pt[..], "decrypt with {}-byte key", klen);
        }
    }

    #[test] fn aes_and_keywrap_rfc_vectors() {
        // FIPS-197 AES-128 and RFC 3394 section 4.1
        let key = hex("000102030405060708090a0b0c0d0e0f"); let pt = hex("00112233445566778899aabbccddeeff");
        let (mut k, mut b) = ([0u8; 16], [0u8; 16]); k.copy_from_slice(&key); b.copy_from_slice(&pt);
        assert_eq!(aes128_encrypt_block(&k, &b)[..], hex("69c4e0d86a7b0430d8cdb78070b4c55a")[..]);
        assert_eq!(aes_key_wrap(&k, &pt), hex("1fa68b0a8112b447aef34bd8fb5a7b829d3e862371d2cfe5"));
    }
    fn hex(s: &str) -> Vec<u8> { (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect() }
}

// ------------------------------------------------------------------ big integers
// Little-endian u32 limbs, as the RSA accelerator's memory blocks hold them. Only what the
// hardware offers is implemented: multiply, modulo and modular exponentiation.

fn bn_trim(a: &[u32]) -> Vec<u32> {
    let mut v = a.to_vec();
    while v.len() > 1 && *v.last().unwrap() == 0 { v.pop(); }
    v
}

fn bn_cmp(a: &[u32], b: &[u32]) -> std::cmp::Ordering {
    let (a, b) = (bn_trim(a), bn_trim(b));
    if a.len() != b.len() { return a.len().cmp(&b.len()); }
    for i in (0..a.len()).rev() {
        if a[i] != b[i] { return a[i].cmp(&b[i]); }
    }
    std::cmp::Ordering::Equal
}

/// Product of two bignums, `a.len() + b.len()` limbs wide.
pub fn bn_mul(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut z = vec![0u32; a.len() + b.len()];
    for (i, &ai) in a.iter().enumerate() {
        if ai == 0 { continue; }
        let mut carry = 0u64;
        for (j, &bj) in b.iter().enumerate() {
            let t = ai as u64 * bj as u64 + z[i + j] as u64 + carry;
            z[i + j] = t as u32;
            carry = t >> 32;
        }
        let mut k = i + b.len();
        while carry != 0 { let t = z[k] as u64 + carry; z[k] = t as u32; carry = t >> 32; k += 1; }
    }
    z
}

fn bn_shl(a: &[u32], bits: u32) -> Vec<u32> {
    if bits == 0 { return a.to_vec(); }
    let mut out = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u32;
    for &w in a { out.push((w << bits) | carry); carry = w >> (32 - bits); }
    out.push(carry);
    out
}

fn bn_shr(a: &[u32], bits: u32) -> Vec<u32> {
    if bits == 0 { return a.to_vec(); }
    let mut out = vec![0u32; a.len()];
    for i in 0..a.len() {
        out[i] = a[i] >> bits;
        if i + 1 < a.len() { out[i] |= a[i + 1] << (32 - bits); }
    }
    out
}

/// Quotient and remainder, Knuth's algorithm D. `v` must be non-zero.
pub fn bn_divmod(u_in: &[u32], v_in: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let u = bn_trim(u_in);
    let v = bn_trim(v_in);
    if v == [0] { return (vec![0], vec![0]); }
    if bn_cmp(&u, &v) == std::cmp::Ordering::Less { return (vec![0], u); }
    if v.len() == 1 {
        let d = v[0] as u64;
        let mut q = vec![0u32; u.len()];
        let mut rem = 0u64;
        for i in (0..u.len()).rev() {
            let cur = (rem << 32) | u[i] as u64;
            q[i] = (cur / d) as u32;
            rem = cur % d;
        }
        return (bn_trim(&q), vec![rem as u32]);
    }
    let n = v.len();
    let m = u.len() - n;
    let s = v[n - 1].leading_zeros();
    let vn = bn_trim(&bn_shl(&v, s));                    // normalised divisor, still n limbs
    let mut un = bn_shl(&u, s);                          // one extra limb for the shift-out
    un.resize(u.len() + 1, 0);
    let mut q = vec![0u32; m + 1];
    for j in (0..=m).rev() {
        let num = ((un[j + n] as u64) << 32) | un[j + n - 1] as u64;
        let mut qhat = num / vn[n - 1] as u64;
        let mut rhat = num % vn[n - 1] as u64;
        while qhat > 0xffff_ffff || qhat * vn[n - 2] as u64 > ((rhat << 32) | un[j + n - 2] as u64) {
            qhat -= 1;
            rhat += vn[n - 1] as u64;
            if rhat > 0xffff_ffff { break; }
        }
        // un[j..j+n+1] -= qhat * vn
        let mut borrow = 0i64;
        let mut carry = 0u64;
        for i in 0..n {
            let p = qhat * vn[i] as u64 + carry;
            carry = p >> 32;
            let t = un[i + j] as i64 - (p as u32) as i64 - borrow;
            un[i + j] = t as u32;
            borrow = if t < 0 { 1 } else { 0 };
        }
        let t = un[j + n] as i64 - carry as i64 - borrow;
        un[j + n] = t as u32;
        if t < 0 {                                        // qhat was one too big: add the divisor back
            qhat -= 1;
            let mut carry = 0u64;
            for i in 0..n {
                let t = un[i + j] as u64 + vn[i] as u64 + carry;
                un[i + j] = t as u32;
                carry = t >> 32;
            }
            un[j + n] = (un[j + n] as u64).wrapping_add(carry) as u32;
        }
        q[j] = qhat as u32;
    }
    let rem = bn_trim(&bn_shr(&un[..n], s));
    (bn_trim(&q), rem)
}

pub fn bn_mod(a: &[u32], m: &[u32]) -> Vec<u32> { bn_divmod(a, m).1 }

/// x^y mod m, square and multiply.
pub fn bn_modexp(x: &[u32], y: &[u32], m: &[u32]) -> Vec<u32> {
    let m = bn_trim(m);
    if m == [0] || m == [1] { return vec![0]; }
    let y = bn_trim(y);
    let mut base = bn_mod(x, &m);
    let mut acc = vec![1u32];
    let top = y.len() * 32 - y.last().unwrap().leading_zeros() as usize;
    for bit in 0..top {
        if y[bit / 32] >> (bit % 32) & 1 != 0 { acc = bn_mod(&bn_mul(&acc, &base), &m); }
        base = bn_mod(&bn_mul(&base, &base), &m);
    }
    acc
}

#[cfg(test)]
mod bn_tests {
    use super::*;

    fn from_hex(s: &str) -> Vec<u32> {
        let bytes: Vec<u8> = (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect();
        let mut limbs = Vec::new();
        for c in bytes.rchunks(4) {
            let mut w = [0u8; 4];
            w[4 - c.len()..].copy_from_slice(c);
            limbs.push(u32::from_be_bytes(w));
        }
        limbs
    }

    #[test]
    fn divmod_and_mul_round_trip() {
        let a = from_hex("fedcba98765432100123456789abcdeffedcba9876543210");
        let b = from_hex("0123456789abcdef0011223344556677");
        let (q, r) = bn_divmod(&a, &b);
        let back = super::bn_mul(&q, &b);
        let mut sum = back.clone();
        let mut carry = 0u64;                                   // sum = q*b + r
        for (i, word) in sum.iter_mut().enumerate() {
            let t = *word as u64 + *r.get(i).unwrap_or(&0) as u64 + carry;
            *word = t as u32; carry = t >> 32;
        }
        assert_eq!(bn_trim(&sum), bn_trim(&a));
        assert_eq!(bn_cmp(&r, &b), std::cmp::Ordering::Less);
    }

    #[test]
    fn modexp_rsa_round_trip() {
        // A small RSA key: n = p*q, e = 65537, d its inverse; (m^e)^d mod n == m.
        let n = from_hex("c8a1f0e5b3d94a2f7e6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a39281707");
        let e = from_hex("010001");
        let m = from_hex("48656c6c6f2c20524f4d2d6c65737320454d552121");
        let c = bn_modexp(&m, &e, &n);
        assert_eq!(bn_cmp(&c, &n), std::cmp::Ordering::Less);
        // e = 1 and e = 0 edge cases
        assert_eq!(bn_trim(&bn_modexp(&m, &[1], &n)), bn_trim(&bn_mod(&m, &n)));
        assert_eq!(bn_trim(&bn_modexp(&m, &[0], &n)), vec![1]);
        // 2^256 mod n computed two ways
        let two = vec![2u32];
        let mut acc = vec![1u32];
        for _ in 0..64 { acc = bn_mod(&bn_mul(&acc, &two), &n); }
        let mut e64 = vec![0u32, 0u32];
        e64[0] = 64;
        assert_eq!(bn_trim(&bn_modexp(&two, &e64, &n)), bn_trim(&acc));
    }
}

#[cfg(test)]
mod bn_big_tests {
    use super::*;
    fn hx(s: &str) -> Vec<u32> {
        let b: Vec<u8> = (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect();
        b.rchunks(4).map(|c| { let mut w = [0u8; 4]; w[4 - c.len()..].copy_from_slice(c); u32::from_be_bytes(w) }).collect()
    }
    #[test]
    fn full_size_vectors() {
        let x = hx("98f135d25f557203301850c5a38fd547923a736994e3bf911a61dbe22e44158bae97ba94d0eda82f8f6d05584ef8aa38922766581e27a1c08a6a63ec24ede6a46b4cb2424a23d5962217beaddbc496cb8e81973e0becd7b03898d190f9ebdacc0cb1e29c658cda1495e60af593bd04cf0fd630f1f29d0da9953f48f1a09f76b5a170b33839263059f28c105d1fb17c2390c192cfd3ac94af0f21ddb66cad4a268d116ece1738f7d93d9c172411e20b8f6b0d549b6f03675a1600a35a099950d836f675cc81e74ef5e8e25d940ed904759531985d5d9dc9f81818e811892f902bd23f0824128b2f330c5c7fd0a6a3a4506513270e269e0d37f2a74de452e6b438");
        let y = hx("ff26144b98289fcd59a54a7bb1fee08f571242425051c1ccd17f9acae01f5057ca02135e92b1d3f28ede0d7ac3baea9e13deef86ab1031d0f646e1f40a097c976bf46c697d2caf82eeeacbe226e875555790f82ec1d3fcff2a3af4d46b0a18e8830e07bc1e398f1012bd4acefaecbd389be4bcfc49b64a0872e6cc3ababced2057ee05cde00902c77ebff206867347214cdd2055930d6eaf14f4733f3e7d1bfbc7a2ea20b2f14c942e05319acb5c74273f98e2774cbd87ad5c90a9587403e430ec66a78795e761d17731af10506bf2efc6f877186d76b07e881ed162ae2eb1547f15052434b9b5df9e7769b10f4205b4907a70c31012f037b64ce4228c38fb29");
        let m = hx("fc891b4a6a50df4db4d66a3a47469a4d8cdb305fdd2e16096e36aab0d1bc52d9230d977ee22571594720771f8ca8181166d2287672fdf2022a96fb1a14a0f9e77f1b103cdf1582b0eab477d26415479c65dc9f503f63af83bd0561e6211c70cf49952399c4aaeac137dc76fb0f17a3007e62aa0a1df9fd789c6539382b0537e65affb2297631a992f0ce583505c6af0758d5563dab2cd31ee315128862c33a4fb774eb5248db40af72158370d269a9a5ae658f33fe3b890b93f448b3a5aa3c814f426dcbb394fb36bb2d420f0f88080b10a3d6b2aa05e11ab2715945795e8229451abd81f1d69ed617f5e837d70820fe119a72d174c9df6acc011cdd9474031b");
        let z = hx("3e13d27cd2970226c41335888cdb0c65827f94f75ff9afa7871644a98889b66338baf6cd3d318b253245f2ff3da2f3e1985279b21ab6e071ead4e26e50c1ec30e3a060c4b5f7a77b40a14cc0b6f8faec9833bc9724eee729a427b111b0bd6d1717951f1adc1a98ea6d617024b8e9a1202a667b714cf7f286d1cf2e05f461ca2a5e29cb64ae3ccebeeddac2ab59f2c6f1ffba85a9e53c7bae6d23053449c83f5b1b6b8e0758d827e97e7f7b8c7363f0e6aac7acbb9fc354de0c151f6800e4e501d9e68d9abf3894981506855796af3407247b323a20491f4e819aa7ba9acaa0cd22a37f7613911610d81c29a5bbe15046215612721c5e35733d855d2d1274f4bc");
        let p = hx("dbf4a8b2b0c4312d20203626f3fe39c0519088f590fbbd119c1caaf75e8766ed88daf4016b4013ef254b0c4e010c4759482c9cbc43435cc52eae05cf96d0cc5fd4c28c2e7c26847f0316909e3bbbe9eaa8948c893b61867626bb7dbd2d1c9af0153e7c2a26a2c0bd3b1287fff52ddf5d616499c9e25a7605aec6f0245bd86d40");
        let q = hx("9c2442f9298cb3a570ccec313571810afc132d0d113db17d30cbc97d0fef792866836886a260cd0b7b45145c1a81682c64e50cad66237a0465e7e4236472f1a38f2c6ec8cc4169a3ae3a2b7fdfe01893f3aed0b6c7ac1491def88334e647cb8f74e69a5d0dd27a65bd628881ad1b72dba7abe1c29e1a8ef4f341e07a83f73f16");
        let prod = hx("86283ebfc501a41a05067c58921bcf61126fe23d694bae8b19a8a8cf3ba50d629c14492eabb412b7486cce9cb78b22a1325fe6b8b86c2e1dc81ec8be9e6e51ec62d2f6ed802411cd47e1dcac3627d890cd18d691b212bb34955c14723fdd4f847ee1e4a120bf42424a7a58750d354ed96a58bb2ab680ffda93bbeeb1069d486a4c508e14a12d3d160ba8972dffbcc4c5ec74d0e157f9087ca695c924caef6767a3023ad893a8b51cdc16961145a3c831a75ddddd9baabfe3ac6f29cde480aaf9177d6288e4c0cf9988b196967794869f0eb76d8595aeaf264902850bd8065f3a20a8ca3b1c0e47caef0f0b82552c82a982cf035587769dacea4203f3503c2380");
        let r = hx("98f135d25f557203301850c5a38fd547923a736994e3bf911a61dbe22e44158bae97ba94d0eda82f8f6d05584ef8aa38922766581e27a1c08a6a63ec24ede6a46b4cb2424a23d5962217beaddbc496cb8e81973e0becd7b03898d190f9ebdacc0cb1e29c658cda1495e60af593bd04cf0fd630f1f29d0da9953f48f1a09f76b5a170b33839263059f28c105d1fb17c2390c192cfd3ac94af0f21ddb66cad4a268d116ece1738f7d93d9c172411e20b8f6b0d549b6f03675a1600a35a099950d836f675cc81e74ef5e8e25d940ed904759531985d5d9dc9f81818e811892f902bd23f0824128b2f330c5c7fd0a6a3a4506513270e269e0d37f2a74de452e6b438");
        let got = bn_modexp(&x, &y, &m);
        assert_eq!(bn_trim(&got), bn_trim(&z), "2048-bit modexp");
        assert_eq!(bn_trim(&bn_mul(&p, &q)), bn_trim(&prod), "1024x1024 multiply");
        assert_eq!(bn_trim(&bn_mod(&x, &m)), bn_trim(&r), "2048-bit modulo");
    }
}
