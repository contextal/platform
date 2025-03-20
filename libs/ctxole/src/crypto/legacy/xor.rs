//! XOR obfuscation

use super::LegacyKey;

/// Xor obfuscation method
#[derive(Debug, Clone)]
pub enum XorMethod {
    /// Method 1: Excel
    One,
    /// Method 2: Word
    Two,
}

/// Xor obfuscation data
#[derive(Debug, Clone)]
pub struct XorKey {
    key: [u8; 16],
    method: XorMethod,
    cypher_size: usize,
}

impl XorKey {
    const PAD_ARRAY: [u8; 15] = [
        0xbb, 0xff, 0xff, 0xba, 0xff, 0xff, 0xb9, 0x80, 0x00, 0xbe, 0x0f, 0x00, 0xbf, 0x0f, 0x00,
    ];

    /// Validate the provided password using method 1 (Excel) and return the derived obfuscation data
    pub fn method1(password: &str, validator: u16) -> Option<LegacyKey> {
        if password.is_empty() {
            return None;
        }
        let password = password[0..(password.len().min(15))].as_bytes();
        if Self::verifier_method1(password) == validator {
            Some(LegacyKey::XorObfuscation(Self {
                key: Self::xor_array_method1(password),
                method: XorMethod::One,
                cypher_size: 0,
            }))
        } else {
            None
        }
    }

    /// Validate the provided password using method 2 (Word) and return the derived obfuscation data
    pub fn method2(password: &str, validator: u32) -> Option<LegacyKey> {
        if password.is_empty() {
            return None;
        }
        // Historically this key derivation mechansim has had two machanics:
        // 8-bit locale-dependent and UTF-16 based
        // Since passwords tend to be mostly ASCII, only the second approach is
        // used here which, in the case of ASCII, covers both variants
        let mut pbytes: Vec<u8> = Vec::with_capacity(15);
        for utf16_char in password.encode_utf16() {
            let lo = utf16_char as u8;
            if lo != 0 {
                // Push low byte if non-zero
                pbytes.push(lo);
            } else {
                // Push high byte otherwise
                pbytes.push((utf16_char >> 8) as u8);
            }
            if pbytes.len() >= 15 {
                break;
            }
        }
        let key = Self::key_method1(pbytes.as_slice());
        let vrf = Self::verifier_method1(pbytes.as_slice());
        if validator == (u32::from(key) << 16) | u32::from(vrf) {
            Some(LegacyKey::XorObfuscation(Self {
                key: Self::xor_array_method2(key, pbytes.as_slice()),
                method: XorMethod::Two,
                cypher_size: 0,
            }))
        } else {
            None
        }
    }

    pub(super) fn apply(&self, block: &mut [u8], position: u64) {
        let mut position = match self.method {
            XorMethod::One => (position as usize + self.cypher_size) & 0xf,
            XorMethod::Two => position as usize & 0xf,
        };
        for v in block.iter_mut() {
            let k = self.key[position];
            match self.method {
                XorMethod::One => {
                    let decr = *v ^ k;
                    let decr = decr.rotate_left(3);
                    *v = decr;
                }
                XorMethod::Two => {
                    if *v != 0 && k != 0 {
                        *v ^= k;
                    }
                }
            }
            position = (position + 1) & 0xf;
        }
    }

    /// Set the size of the method1 encrypted data
    ///
    /// Due to the way XOR obfuscation method 1 applies the key, the total encrypted length
    /// must be known in advance. This is the way to set it
    pub fn set_method1_cypher_size(&mut self, size: usize) {
        if matches!(self.method, XorMethod::One) {
            self.cypher_size = size;
        }
    }

    fn verifier_method1(password: &[u8]) -> u16 {
        assert!((1..16).contains(&password.len()));
        let len = password.len();
        let mut verifier = 0u16;
        for c in password.iter().rev().chain(&[len as u8]) {
            let imd1: u16 = verifier >> 14 & 1;
            let imd2: u16 = (verifier << 1) & 0x7fff;
            let imd3: u16 = imd1 | imd2;
            verifier = imd3 ^ u16::from(*c);
        }
        verifier ^ 0xce4b
    }

    fn key_method1(password: &[u8]) -> u16 {
        assert!((1..16).contains(&password.len()));
        const XOR_MATRIX: [u16; 105] = [
            0xaefc, 0x4dd9, 0x9bb2, 0x2745, 0x4e8a, 0x9d14, 0x2a09, 0x7b61, 0xf6c2, 0xfda5, 0xeb6b,
            0xc6f7, 0x9dcf, 0x2bbf, 0x4563, 0x8ac6, 0x05ad, 0x0b5a, 0x16b4, 0x2d68, 0x5ad0, 0x0375,
            0x06ea, 0x0dd4, 0x1ba8, 0x3750, 0x6ea0, 0xdd40, 0xd849, 0xa0b3, 0x5147, 0xa28e, 0x553d,
            0xaa7a, 0x44d5, 0x6f45, 0xde8a, 0xad35, 0x4a4b, 0x9496, 0x390d, 0x721a, 0xeb23, 0xc667,
            0x9cef, 0x29ff, 0x53fe, 0xa7fc, 0x5fd9, 0x47d3, 0x8fa6, 0x0f6d, 0x1eda, 0x3db4, 0x7b68,
            0xf6d0, 0xb861, 0x60e3, 0xc1c6, 0x93ad, 0x377b, 0x6ef6, 0xddec, 0x45a0, 0x8b40, 0x06a1,
            0x0d42, 0x1a84, 0x3508, 0x6a10, 0xaa51, 0x4483, 0x8906, 0x022d, 0x045a, 0x08b4, 0x1168,
            0x76b4, 0xed68, 0xcaf1, 0x85c3, 0x1ba7, 0x374e, 0x6e9c, 0x3730, 0x6e60, 0xdcc0, 0xa9a1,
            0x4363, 0x86c6, 0x1dad, 0x3331, 0x6662, 0xccc4, 0x89a9, 0x0373, 0x06e6, 0x0dcc, 0x1021,
            0x2042, 0x4084, 0x8108, 0x1231, 0x2462, 0x48c4,
        ];
        const INITIAL_CODE: [u16; 15] = [
            0xe1f0, 0x1d0f, 0xcc9c, 0x84c0, 0x110c, 0x0e10, 0xf1ce, 0x313e, 0x1872, 0xe139, 0xd40f,
            0x84f9, 0x280c, 0xa96a, 0x4ec3,
        ];
        let mut key = INITIAL_CODE[password.len() - 1];
        let mut current_element = 0x68 + 1;
        for c in password.iter().rev() {
            let mut c = *c;
            for _ in 0..7 {
                current_element -= 1;
                if c & 0x40 != 0 {
                    key ^= XOR_MATRIX[current_element];
                }
                c <<= 1;
            }
        }
        key
    }

    fn xor_ror(a: u8, b: u8) -> u8 {
        let tmp = a ^ b;
        tmp.rotate_right(1)
    }

    fn xor_array_method1(password: &[u8]) -> [u8; 16] {
        assert!((1..16).contains(&password.len()));
        let key = Self::key_method1(password);
        let key_hi = (key >> 8) as u8;
        let key_lo = key as u8;
        let mut index = 0;
        let mut it = password.iter().chain(Self::PAD_ARRAY.iter());
        let mut obfuscation_array = [0u8; 16];
        while index < 16 {
            obfuscation_array[index] = Self::xor_ror(*it.next().unwrap(), key_lo);
            obfuscation_array[index + 1] = Self::xor_ror(*it.next().unwrap(), key_hi);
            index += 2;
        }
        obfuscation_array
    }

    fn xor_array_method2(key: u16, password: &[u8]) -> [u8; 16] {
        assert!((1..16).contains(&password.len()));
        let key_hi = (key >> 8) as u8;
        let key_lo = key as u8;
        let password_len = password.len();
        let mut obfuscation_array = [0u8; 16];
        obfuscation_array[0..password_len].copy_from_slice(password);
        obfuscation_array[password_len..16]
            .copy_from_slice(&Self::PAD_ARRAY[0..(16 - password_len)]);
        for word in obfuscation_array.chunks_exact_mut(2) {
            let tmp = word[0] ^ key_lo;
            word[0] = tmp.rotate_right(1);
            let tmp = word[1] ^ key_hi;
            word[1] = tmp.rotate_right(1);
        }
        obfuscation_array
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn method1() {
        assert_eq!(XorKey::key_method1(b"contextal"), 0x3702);
        assert_eq!(XorKey::key_method1(b"123456789abcdef"), 0xc378);
        assert!(XorKey::method1("", 0).is_none());
        for (p, vrfy, arr) in [
            (
                "even",
                0xca95,
                b"\x96\x02\x96\x0e\xf9\xc6\xdbd\xdb\xc6\xf8y$f\xa39",
            ),
            (
                "odd",
                0xcc26,
                b"\xb2z7\x95\xfa\xb7X\xb7\xfa\x94EHZ\xcf\x05\x97",
            ),
            ("contextal", 0xdaa9, b"\xb0,6\xa1\xb3\xa7;+7F\xfed\\d\xfeG"),
            (
                "1",
                0xce28,
                b"\xd9>\xbe\x1c\x1c\x1c\xbe?\x01\xe3\x1edA<\xc6\xe3",
            ),
            (
                "0123456789abcde",
                0xb7c4,
                b"\xa5\x0e\xa4\x0f\xa7\x0c\xa6\r\xa1\n\r\xa7\x0c\xa4\x0fK",
            ),
            ("0123456789abcd", 0xb7a0, b"W'V&U%T$S#\xff\x8e\xfe\x8d\x92@"),
            (
                "0123456789abcde",
                0xb7c4,
                b"\xa5\x0e\xa4\x0f\xa7\x0c\xa6\r\xa1\n\r\xa7\x0c\xa4\x0fK",
            ),
            (
                "0123456789abcdefgh",
                0xb7c4,
                b"\xa5\x0e\xa4\x0f\xa7\x0c\xa6\r\xa1\n\r\xa7\x0c\xa4\x0fK",
            ),
        ] {
            if let LegacyKey::XorObfuscation(XorKey {
                key,
                method,
                cypher_size: 0,
            }) = XorKey::method1(p, vrfy).unwrap_or_else(|| panic!("verify fail for \"{p}\""))
            {
                assert!(matches!(method, XorMethod::One), "wrong method on \"{p}\"");
                assert_eq!(key.as_slice(), arr, "wrong array on \"{p}\"");
            } else {
                panic!("unexpected result for \"{p}\"");
            }
        }

        let key = XorKey {
            key: [
                42, 88, 43, 91, 40, 90, 41, 93, 46, 89, 42, 88, 43, 91, 40, 156,
            ],
            method: XorMethod::One,
            cypher_size: 11,
        };
        const CYPHER: [u8; 11] = [
            0xacu8, 0x05, 0x00, 0x00, 0x5b, 0x28, 0xfc, 0x2a, 0xde, 0x6d, 0x5d,
        ];
        let mut data = CYPHER;
        key.apply(&mut data, 1374);
        assert_eq!(
            data.as_slice(),
            &[
                0xaf, 0x79, 0xc2, 0x59, 0x00, 0x00, 0x03, 0x00, 0x34, 0x32, 0x30
            ]
        );
        let mut data = CYPHER;
        key.apply(&mut data[1..], 1375);
        assert_eq!(
            &data[1..],
            &[0x79, 0xc2, 0x59, 0x00, 0x00, 0x03, 0x00, 0x34, 0x32, 0x30]
        );
    }
}
