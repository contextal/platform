//! Win32 structs and fns
#[cfg(feature = "serde")]
use serde::{Serialize, Serializer};
use std::fmt::{self, Debug, Display};
use std::io::{self, Read};
use std::str::FromStr;

/// A Win32 GUID
#[derive(PartialEq, Eq, Clone)]
pub struct GUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

impl GUID {
    /// Create a null (all zeroes) GUID
    pub fn null() -> Self {
        Self {
            data1: 0,
            data2: 0,
            data3: 0,
            data4: [0u8; 8],
        }
    }

    /// Create a GUID from 16 raw bytes
    pub fn from_le_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 16 {
            return None;
        }
        let data1 = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let data2 = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        let data3 = u16::from_le_bytes(bytes[6..8].try_into().unwrap());
        let data4: [u8; 8] = bytes[8..16].try_into().unwrap();
        Some(Self {
            data1,
            data2,
            data3,
            data4,
        })
    }

    /// Create a GUID from a [`Read`] stream
    pub fn from_le_stream<R: Read>(f: &mut R) -> Result<Self, io::Error> {
        let mut guid = [0u8; 16];
        f.read_exact(&mut guid)?;
        Ok(Self::from_le_bytes(&guid).unwrap())
    }

    /// Check whether the GUID is null
    pub fn is_null(&self) -> bool {
        *self == Self::null()
    }
}

impl Display for GUID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            self.data1,
            self.data2,
            self.data3,
            self.data4[0],
            self.data4[1],
            self.data4[2],
            self.data4[3],
            self.data4[4],
            self.data4[5],
            self.data4[6],
            self.data4[7]
        )
    }
}

#[cfg(feature = "serde")]
impl Serialize for GUID {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl Debug for GUID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}}}", self)
    }
}

impl Default for GUID {
    fn default() -> Self {
        Self::null()
    }
}

impl FromStr for GUID {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.as_bytes();
        if s.len() != 36 {
            return Err(());
        }
        if s[8] != b'-' || s[13] != b'-' || s[18] != b'-' || s[23] != b'-' {
            return Err(());
        }
        let le_bytes: [u8; 16] = [
            hex_decode_byte(&s[6..8])?,
            hex_decode_byte(&s[4..6])?,
            hex_decode_byte(&s[2..4])?,
            hex_decode_byte(&s[0..2])?,
            hex_decode_byte(&s[11..13])?,
            hex_decode_byte(&s[9..11])?,
            hex_decode_byte(&s[16..18])?,
            hex_decode_byte(&s[14..16])?,
            hex_decode_byte(&s[19..21])?,
            hex_decode_byte(&s[21..23])?,
            hex_decode_byte(&s[24..26])?,
            hex_decode_byte(&s[26..28])?,
            hex_decode_byte(&s[28..30])?,
            hex_decode_byte(&s[30..32])?,
            hex_decode_byte(&s[32..34])?,
            hex_decode_byte(&s[34..36])?,
        ];
        Self::from_le_bytes(&le_bytes).ok_or(())
    }
}

#[inline]
fn hex_decode_nibble(c: u8) -> Result<u8, ()> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(()),
    }
}

#[inline]
fn hex_decode_byte(s: &[u8]) -> Result<u8, ()> {
    if s.len() != 2 {
        return Err(());
    }
    Ok((hex_decode_nibble(s[0])? << 4) | hex_decode_nibble(s[1])?)
}

/// Translates a windows FILETIME to a [datetime](time::OffsetDateTime)
///
/// Returns None if the date is out of range
pub fn filetime_to_datetime(ftime: u64) -> Option<time::OffsetDateTime> {
    let ftime = i128::from(ftime);
    let ftime = ftime.checked_sub(116444736000000000)?;
    time::OffsetDateTime::from_unix_timestamp_nanos(ftime * 100).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_nibble() {
        assert_eq!(hex_decode_nibble(b'0'), Ok(0));
        assert_eq!(hex_decode_nibble(b'1'), Ok(1));
        assert_eq!(hex_decode_nibble(b'9'), Ok(9));
        assert_eq!(hex_decode_nibble(b'A'), Ok(0xa));
        assert_eq!(hex_decode_nibble(b'B'), Ok(0xb));
        assert_eq!(hex_decode_nibble(b'F'), Ok(0xf));
        assert_eq!(hex_decode_nibble(b'a'), Ok(0xa));
        assert_eq!(hex_decode_nibble(b'b'), Ok(0xb));
        assert_eq!(hex_decode_nibble(b'f'), Ok(0xf));
        assert_eq!(hex_decode_nibble(b'q'), Err(()));
        assert_eq!(hex_decode_nibble(b')'), Err(()));
    }

    #[test]
    fn test_decode_bytes() {
        assert_eq!(hex_decode_byte(b"aC"), Ok(0xac));
        assert_eq!(hex_decode_byte(b"Ab"), Ok(0xab));
        assert_eq!(hex_decode_byte(b"13"), Ok(0x13));
        assert_eq!(hex_decode_byte(b"37"), Ok(0x37));
        assert_eq!(hex_decode_byte(b"-A"), Err(()));
        assert_eq!(hex_decode_byte(b"1234"), Err(()));
        assert_eq!(hex_decode_byte(b"d0"), Ok(0xd0));
    }

    #[test]
    fn test_guid() {
        assert_eq!(
            GUID::null(),
            GUID {
                data1: 0,
                data2: 0,
                data3: 0,
                data4: [0u8; 8]
            }
        );
        assert_eq!(GUID::from_le_bytes(&[0u8; 16]), Some(GUID::default()));
        let guid = GUID::from_le_bytes(&[
            0x53, 0xff, 0x4b, 0x99, 0xf9, 0xdd, 0xad, 0x42, 0xa5, 0x6a, 0xff, 0xea, 0x36, 0x17,
            0xac, 0x16,
        ])
        .unwrap();
        assert_eq!(guid.data1, 0x994bff53);
        assert_eq!(guid.data2, 0xddf9);
        assert_eq!(guid.data3, 0x42ad);
        assert_eq!(guid.data4, [0xa5, 0x6a, 0xff, 0xea, 0x36, 0x17, 0xac, 0x16]);
        assert_eq!(guid.to_string(), "994bff53-ddf9-42ad-a56a-ffea3617ac16");
        assert_ne!(guid, GUID::null());
        assert_eq!(
            GUID::from_str("994BFF53-ddf9-42AD-a56a-FFEA3617AC16").unwrap(),
            guid
        );
        assert_eq!(
            GUID::from_str("00000000-0000-0000-0000-000000000000").unwrap(),
            GUID::default()
        );
        assert_eq!(GUID::from_le_bytes(&[1, 2, 3, 4]), None);
        assert_eq!(GUID::from_le_bytes(&[0u8; 17]), None);
        assert_eq!(
            GUID::from_str("00000000-0000-0000-0000-00000000000"),
            Err(())
        );
        assert_eq!(
            GUID::from_str("00000000-0000-0000-000000-0000000000"),
            Err(())
        );
        assert_eq!(
            GUID::from_str("00000000-0000 0000-0000-00000000000"),
            Err(())
        );
        assert_eq!(
            GUID::from_str("00000000-000000-00-0000-00000000000"),
            Err(())
        );
        assert_eq!(
            GUID::from_str("00000000_0000-0000-0000-00000000000"),
            Err(())
        );
    }

    #[test]
    fn test_filetime_to_datetime() -> Result<(), io::Error> {
        assert_eq!(
            filetime_to_datetime(0x01BAB44B12F98800).unwrap(),
            time::OffsetDateTime::new_utc(
                time::Date::from_calendar_date(1995, time::Month::November, 16).unwrap(),
                time::Time::from_hms(17, 43, 44).unwrap()
            )
        );
        Ok(())
    }
}
