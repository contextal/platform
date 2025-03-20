//! Traditional PKWARE and WinZip encryption
use ctr::cipher::{KeyIvInit, StreamCipher};
use hmac::{Mac, SimpleHmac, digest::FixedOutputReset};
use std::{
    cell::RefCell,
    io::{Read, Seek},
    rc::Rc,
};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, trace, warn};

/// Traditional PKWARE encryption - Encryption data
#[derive(Debug)]
pub struct PkEncryption {
    /// Encryption header used in verification of provided password.
    hdr: [u8; 12],
    /// A variant holding a value for a decryption check. It is used to distinguish invalid
    /// passwords from "seemingly valid" passwords without decrypting the whole encrypted stream.
    chk: PkPwdCheck,
    /// One of three encryption keys.
    k0: u32,
    /// One of three encryption keys.
    k1: u32,
    /// One of three encryption keys.
    k2: u32,
}

impl PkEncryption {
    /// Try to read the encryption header and construct a `PkEncryption` instance.
    pub(crate) fn new<R: Read>(r: &mut R, pwdcheck: PkPwdCheck) -> Result<Self, std::io::Error> {
        let mut hdr = [0u8; 12];
        r.read_exact(&mut hdr)?;
        Ok(Self {
            hdr,
            chk: pwdcheck,
            k0: 0,
            k1: 0,
            k2: 0,
        })
    }

    /// Mixes a byte in with the 3 keys
    fn update_keys(&mut self, c: u8) {
        self.k0 = crc32_lut(self.k0, c);
        self.k1 = self.k1.wrapping_add(self.k0 & 0xff);
        self.k1 = self.k1.wrapping_mul(134775813).wrapping_add(1);
        self.k2 = crc32_lut(self.k2, (self.k1 >> 24) as u8);
    }

    /// Decrypts a buffer and updates/maintains encryption keys state.
    fn decrypt(&mut self, buf: &mut [u8]) {
        for c in buf.iter_mut() {
            let temp = (self.k2 | 2) as u16;
            let d = ((temp.wrapping_mul(temp ^ 1)) >> 8) as u8;
            *c ^= d;
            self.update_keys(*c);
        }
    }

    /// Tests the given password against the encryption data
    fn set_password(&mut self, password: &[u8]) -> bool {
        self.k0 = 305419896;
        self.k1 = 591751049;
        self.k2 = 878082192;
        for c in password {
            self.update_keys(*c);
        }
        let mut buf = self.hdr;
        self.decrypt(&mut buf);
        let found = match self.chk {
            PkPwdCheck::Byte(c) => buf[11] == c,
            PkPwdCheck::Word(c) => buf[10] == c as u8 && buf[11] == (c >> 8) as u8,
        };
        if found {
            debug!(
                "Pk key found ({:08x} {:08x} {:08x})",
                self.k0, self.k1, self.k2
            );
        }
        found
    }
}

/// PKWARE encryption - Check type (single byte or two bytes)
#[derive(Debug)]
pub(crate) enum PkPwdCheck {
    /// A variant representing and containing a check type of single byte length.
    Byte(u8),
    /// A variant representing and containing a check type of two bytes length.
    Word(u16),
}

/// PKWARE encryption - decrypting stream reader
struct PkEncryptionStream<R: Read> {
    /// A generic type which implements `Read` trait.
    r: R,
    /// Ref-counted instance of `PkEncryption`, which is shared with an `Entry` instance.
    enc: Rc<RefCell<PkEncryption>>,
}

impl<R: Read> PkEncryptionStream<R> {
    /// Creates a new instance of decrypting stream reader.
    pub(crate) fn new(r: R, enc: Rc<RefCell<PkEncryption>>) -> Self {
        Self { r, enc }
    }
}

impl<R: Read> Read for PkEncryptionStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let size = self.r.read(buf)?;
        self.enc.borrow_mut().decrypt(&mut buf[0..size]);
        Ok(size)
    }
}

/// PKWARE encryption - lookup table based crc32 calculator
///
/// Using the fast hasher is actually slower here
fn crc32_lut(crc: u32, b: u8) -> u32 {
    CRC32_TABLE[((crc as u8) ^ b) as usize] ^ (crc >> 8)
}

// Generated via:
// POLYNOMIAL = 0xedb88320
// table = []
// for b in range(256):
//   rmd = b
//   for bit in range(8, 0, -1):
//     if rmd & 1 != 0:
//         rmd = (rmd >> 1) ^ POLYNOMIAL
//     else:
//         rmd = rmd >> 1
//   table.append(rmd)
const CRC32_TABLE: [u32; 256] = [
    0, 1996959894, 3993919788, 2567524794, 124634137, 1886057615, 3915621685, 2657392035,
    249268274, 2044508324, 3772115230, 2547177864, 162941995, 2125561021, 3887607047, 2428444049,
    498536548, 1789927666, 4089016648, 2227061214, 450548861, 1843258603, 4107580753, 2211677639,
    325883990, 1684777152, 4251122042, 2321926636, 335633487, 1661365465, 4195302755, 2366115317,
    997073096, 1281953886, 3579855332, 2724688242, 1006888145, 1258607687, 3524101629, 2768942443,
    901097722, 1119000684, 3686517206, 2898065728, 853044451, 1172266101, 3705015759, 2882616665,
    651767980, 1373503546, 3369554304, 3218104598, 565507253, 1454621731, 3485111705, 3099436303,
    671266974, 1594198024, 3322730930, 2970347812, 795835527, 1483230225, 3244367275, 3060149565,
    1994146192, 31158534, 2563907772, 4023717930, 1907459465, 112637215, 2680153253, 3904427059,
    2013776290, 251722036, 2517215374, 3775830040, 2137656763, 141376813, 2439277719, 3865271297,
    1802195444, 476864866, 2238001368, 4066508878, 1812370925, 453092731, 2181625025, 4111451223,
    1706088902, 314042704, 2344532202, 4240017532, 1658658271, 366619977, 2362670323, 4224994405,
    1303535960, 984961486, 2747007092, 3569037538, 1256170817, 1037604311, 2765210733, 3554079995,
    1131014506, 879679996, 2909243462, 3663771856, 1141124467, 855842277, 2852801631, 3708648649,
    1342533948, 654459306, 3188396048, 3373015174, 1466479909, 544179635, 3110523913, 3462522015,
    1591671054, 702138776, 2966460450, 3352799412, 1504918807, 783551873, 3082640443, 3233442989,
    3988292384, 2596254646, 62317068, 1957810842, 3939845945, 2647816111, 81470997, 1943803523,
    3814918930, 2489596804, 225274430, 2053790376, 3826175755, 2466906013, 167816743, 2097651377,
    4027552580, 2265490386, 503444072, 1762050814, 4150417245, 2154129355, 426522225, 1852507879,
    4275313526, 2312317920, 282753626, 1742555852, 4189708143, 2394877945, 397917763, 1622183637,
    3604390888, 2714866558, 953729732, 1340076626, 3518719985, 2797360999, 1068828381, 1219638859,
    3624741850, 2936675148, 906185462, 1090812512, 3747672003, 2825379669, 829329135, 1181335161,
    3412177804, 3160834842, 628085408, 1382605366, 3423369109, 3138078467, 570562233, 1426400815,
    3317316542, 2998733608, 733239954, 1555261956, 3268935591, 3050360625, 752459403, 1541320221,
    2607071920, 3965973030, 1969922972, 40735498, 2617837225, 3943577151, 1913087877, 83908371,
    2512341634, 3803740692, 2075208622, 213261112, 2463272603, 3855990285, 2094854071, 198958881,
    2262029012, 4057260610, 1759359992, 534414190, 2176718541, 4139329115, 1873836001, 414664567,
    2282248934, 4279200368, 1711684554, 285281116, 2405801727, 4167216745, 1634467795, 376229701,
    2685067896, 3608007406, 1308918612, 956543938, 2808555105, 3495958263, 1231636301, 1047427035,
    2932959818, 3654703836, 1088359270, 936918000, 2847714899, 3736837829, 1202900863, 817233897,
    3183342108, 3401237130, 1404277552, 615818150, 3134207493, 3453421203, 1423857449, 601450431,
    3009837614, 3294710456, 1567103746, 711928724, 3020668471, 3272380065, 1510334235, 755167117,
];

/// WinZip AE-1 and AE-2 encryption - Encryption data
///
/// See <https://www.winzip.com/en/support/aes-encryption/>
#[derive(Debug)]
pub struct WzEncryption {
    /// A structure holding misc fields describing WinZip encryption found in the extra field.
    pub extra_fields: WzExtraFields,
    /// A buffer to store a salt value. Salt itself resides in the first `salt_len` bytes of the
    /// array.
    salt: [u8; 16],
    /// Actual salt length in bytes.
    salt_len: usize,
    /// Result of cryptographic hash function, which is used to distinguish invalid passwords from
    /// "seemingly valid" ones without performing full stream decryption.
    verify: [u8; 2],
    /// A placeholder to store results of cryptographic hash function performed over provided
    /// password and salt. Parts of the buffer are interpreted as a "crypt key", "sign key" and a
    /// value which should match to `verify` field if provided password is correct.
    derived: [u8; 66],
    /// Delineates used part of `derived` buffer from the unused part. The size of used part of the
    /// buffer depends on used encryption block cypher strength.
    derived_len: usize,
    /// Supplied result of cryptographic hash function used to verify integrity and authenticity of
    /// fully decrypted data stream.
    auth_code: [u8; 10],
    /// Actual compressed size.
    actual_size: u64,
    /// An interface to calculate and maintain a Message Authentication Code of the data while it
    /// is being extracted.
    mac: SimpleHmac<sha1::Sha1>,
}

impl WzEncryption {
    /// Tries to read and parse the encryption header into a `WzEncryption` instance
    pub(crate) fn new<R: Read + Seek>(
        r: &mut R,
        extradata: &[u8],
        compressed_size: u64,
    ) -> Result<Self, std::io::Error> {
        // Decode extra fields
        let extra_fields = WzExtraFields::new(extradata)?;
        // Set up lengths for the indicated strength
        let salt_len = (extra_fields.strength + 1) * 4;
        let derived_len = (salt_len * 4) + 2;
        // Read fields
        let mut salt = [0u8; 16];
        r.read_exact(&mut salt[0..salt_len.into()])?;
        let mut verify = [0u8; 2];
        r.read_exact(&mut verify)?;
        // Overhead is the len of (salt + verify + auth code)
        let overhead: u64 = (salt_len + 2 + 10).into();
        // This is the *actual compressed size*
        let actual_size = compressed_size.checked_sub(overhead).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "WinZip AES extra field overflow",
            )
        })?;
        // Need to seek back here before returning
        let curpos = r.stream_position()?;
        let skip_to = curpos.checked_add(actual_size).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "WinZip AES file size is too big",
            )
        })?;
        r.seek(std::io::SeekFrom::Start(skip_to))?;
        let mut auth_code = [0u8; 10];
        r.read_exact(&mut auth_code)?;
        r.seek(std::io::SeekFrom::Start(curpos))?;
        Ok(Self {
            extra_fields,
            salt,
            salt_len: salt_len.into(),
            verify,
            derived: [0u8; 66],
            derived_len: derived_len.into(),
            auth_code,
            actual_size,
            mac: SimpleHmac::<sha1::Sha1>::new_from_slice(b"").unwrap(),
        })
    }

    /// Returns a "crypt key" obtained during an invocation of `set_password` method.
    fn get_crypt_key(&self) -> &[u8] {
        &self.derived[0..((self.derived_len - 2) / 2)]
    }

    /// Returns a "sign key" obtained during an invocation of `set_password` method.
    fn get_sign_key(&self) -> &[u8] {
        &self.derived[((self.derived_len - 2) / 2)..(self.derived_len - 2)]
    }

    /// Returns a value which represents an actual compression method.
    pub fn get_actual_compression_method(&self) -> u16 {
        self.extra_fields.actual_method
    }

    /// Returns an actual compressed size in bytes.
    pub fn get_actual_compressed_size(&self) -> u64 {
        self.actual_size
    }

    /// Verifies integrity/authenticity of decrypted data. This method has to be called when all
    /// the encrypted data has been read.
    pub fn check_authentication_code(&mut self) -> bool {
        let mut buf = [0u8; 20];
        self.mac.finalize_into_reset((&mut buf).into());
        buf[0..10] == self.auth_code
    }

    /// Tests the given password against the encryption data. And populates an internal structure
    /// based on provided password.
    fn set_password(&mut self, password: &[u8]) -> bool {
        pbkdf2::pbkdf2_hmac::<sha1::Sha1>(
            password,
            &self.salt[..self.salt_len],
            1000,
            &mut self.derived[0..self.derived_len],
        );
        let found = self.derived[(self.derived_len - 2)..self.derived_len] == self.verify;
        if found {
            debug!(
                "Wz key found ({:02x?}) - Sign key is: ({:02x?})",
                self.get_crypt_key(),
                self.get_sign_key()
            );
            debug!("Wz enc: {:x?}", self);
            self.mac = SimpleHmac::<sha1::Sha1>::new_from_slice(self.get_sign_key()).unwrap();
        }
        found
    }
}

/// WinZip AE-\[12\] encryption - misc fields found in the extra field
#[derive(Debug)]
pub struct WzExtraFields {
    /// Vendor version.
    pub vendor_version: u16,
    /// Vendor identifier.
    pub vendor_id: u16,
    /// Encryption block cypher strength.
    pub strength: u8,
    /// Actual compression method.
    pub actual_method: u16,
}

impl WzExtraFields {
    /// Tries to read, parse and create a new instance of misc fields found in the extra field
    fn new(mut extradata: &[u8]) -> Result<Self, std::io::Error> {
        let (vendor_version, vendor_id, strength, actual_method) = (|| {
            Ok((
                ctxutils::io::rdu16le(&mut extradata)?,
                ctxutils::io::rdu16le(&mut extradata)?,
                ctxutils::io::rdu8(&mut extradata)?,
                ctxutils::io::rdu16le(&mut extradata)?,
            ))
        })()
        .map_err(|_: std::io::Error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "WinZip AES extra field overflow",
            )
        })?;
        if !(0x0001..=0x0002).contains(&vendor_version) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("WinZip AES with unknown version ({:04x})", vendor_version),
            ));
        }
        if vendor_id != 0x4541 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("WinZip AES with unknown vendor id ({:04x})", vendor_id),
            ));
        }
        if !(0x01..=0x03).contains(&strength) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("WinZip AES with unknown strength ({})", strength),
            ));
        }
        Ok(Self {
            vendor_version,
            vendor_id,
            strength,
            actual_method,
        })
    }
}

/// WinZip AE-\[12\] encryption - decrypting stream reader
struct WzEncryptionStream<R: Read> {
    /// A generic type which implements `Read` trait.
    r: R,
    /// Ref-counted instance of `WzEncryption`, shared with an `Entry` instance.
    enc: Rc<RefCell<WzEncryption>>,
    /// A generic stream cipher used for and updated during decryption
    cypher: Box<dyn StreamCipher>,
}

impl<R: Read> WzEncryptionStream<R> {
    /// Creates a new instance of WinZip encryption decrypting stream reader
    pub fn new(r: R, enc: Rc<RefCell<WzEncryption>>) -> Self {
        let cypher: Box<dyn StreamCipher> = {
            let enc = enc.borrow_mut();
            let key = enc.get_crypt_key();
            let iv = 1u128.to_le_bytes();
            match enc.extra_fields.strength {
                1 => Box::new(ctr::Ctr128LE::<aes::Aes128>::new(
                    key.into(),
                    iv.as_slice().into(),
                )),
                2 => Box::new(ctr::Ctr128LE::<aes::Aes192>::new(
                    key.into(),
                    iv.as_slice().into(),
                )),
                3 => Box::new(ctr::Ctr128LE::<aes::Aes256>::new(
                    key.into(),
                    iv.as_slice().into(),
                )),
                _ => unreachable!(),
            }
        };

        Self { r, enc, cypher }
    }
}

impl<R: Read> Read for WzEncryptionStream<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let size = self.r.read(buf)?;
        self.enc.borrow_mut().mac.update(&buf[0..size]);
        self.cypher.apply_keystream(&mut buf[0..size]);
        Ok(size)
    }
}

/// A generic interface for PKWARE, WinZip or "no-encryption" encryption.
#[derive(Debug, Clone)]
pub enum ZipEncryption {
    /// Traditional PKWARE encryption variant
    Pk(Rc<RefCell<PkEncryption>>),
    /// WinZip AE-1 and AE-2 (AES128, AES192, AES256)
    Wz(Rc<RefCell<WzEncryption>>),
    /// A variant signifying no encryption
    Null,
}

impl ZipEncryption {
    /// Creates a new decrypting stream reader from a provided reader
    pub fn new_stream<'a, R: Read + 'a>(&self, reader: R) -> Box<dyn Read + 'a> {
        match self {
            Self::Wz(wz) => Box::new(WzEncryptionStream::new(reader, wz.clone())),
            Self::Pk(pk) => Box::new(PkEncryptionStream::new(reader, pk.clone())),
            Self::Null => Box::new(reader),
        }
    }

    /// Tests the given password against the encryption data
    pub fn set_password(&self, password: &[u8]) -> bool {
        match self {
            Self::Pk(enc) => enc.borrow_mut().set_password(password),
            Self::Wz(enc) => enc.borrow_mut().set_password(password),
            Self::Null => false,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pk_decrypt() {
        type Cypher = ctr::Ctr128LE<aes::Aes256>;
        let mut encrypted = *b"\x05\x18\x4d\x1f\xfb\xdc\x7b\x30\x89\x61\xd5\xf4\x63\x26\x0e\xf3\x9b\xa9\xb7\xf8\x32\xcb\x31\x5f\x95\x4d\xbc\x1d\x81\x6b\x08\x2c";
        let iv = 1u128.to_le_bytes();
        let key: [u8; 32] = [
            225, 71, 221, 157, 162, 57, 192, 82, 56, 101, 51, 45, 172, 192, 146, 140, 93, 190, 29,
            105, 244, 114, 202, 55, 50, 151, 127, 12, 136, 219, 34, 112,
        ];
        let mut cypher = Cypher::new(key.as_slice().into(), iv.as_slice().into());
        for chunk in encrypted.chunks_mut(16) {
            cypher.apply_keystream(chunk);
        }
        assert_eq!(encrypted.as_slice(), b"0123456789abcdef\nHello world!!1\n");
    }
}
