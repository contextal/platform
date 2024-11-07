use ctxutils::io::*;
use std::cell::RefCell;
use std::io::{self, Read};

pub use PropertyMaskBit::*;

pub enum PropertyMaskBit<'a> {
    U32(RefCell<&'a mut u32>),
    I32(RefCell<&'a mut i32>),
    U16(RefCell<&'a mut u16>),
    I16(RefCell<&'a mut i16>),
    U8(RefCell<&'a mut u8>),
    I8(RefCell<&'a mut i8>),
    Bool(RefCell<&'a mut bool>),
    OU32(RefCell<&'a mut Option<u32>>),
    OI32(RefCell<&'a mut Option<i32>>),
    OU16(RefCell<&'a mut Option<u16>>),
    OI16(RefCell<&'a mut Option<i16>>),
    OU8(RefCell<&'a mut Option<u8>>),
    OI8(RefCell<&'a mut Option<i8>>),
    Flag(RefCell<&'a mut Option<()>>),
    Unused,
}
pub type PropertyMask<'a> = [PropertyMaskBit<'a>; 32];

pub trait IntoPropertyMaskBit {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit;
}

impl IntoPropertyMaskBit for u32 {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        U32(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for i32 {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        I32(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for u16 {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        U16(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for i16 {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        I16(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for u8 {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        U8(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for i8 {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        I8(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for bool {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        Bool(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<u32> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        OU32(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<i32> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        OI32(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<u16> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        OU16(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<i16> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        OI16(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<u8> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        OU8(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<i8> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        OI8(RefCell::new(self))
    }
}

impl IntoPropertyMaskBit for Option<()> {
    fn to_property_bit_mut(&mut self) -> PropertyMaskBit {
        Flag(RefCell::new(self))
    }
}

macro_rules! property_mask_bit {
    ($a:expr) => {
        (&mut $a).to_property_bit_mut()
    };
}

pub(crate) fn set_data_properties<R: Read>(
    f: &mut R,
    prop_ref: &PropertyMask,
) -> Result<Vec<u8>, io::Error> {
    let size = rdu16le(f)?;
    if size < 4 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Insufficient stream size",
        ));
    }
    let prop_mask = rdu32le(f)?;
    let mut buf: Vec<u8> = vec![0u8; (size - 4).into()];
    f.read_exact(&mut buf)?;
    let mut cur = 0usize;
    for (i, prop) in prop_ref.iter().enumerate() {
        if let Flag(d) = prop {
            **d.borrow_mut() = if (prop_mask & (1 << i)) == 0 {
                None
            } else {
                Some(())
            };
            continue;
        }
        if (prop_mask & (1 << i)) == 0 {
            continue;
        }
        match prop {
            U32(d) => {
                cur = cur + ((4 - (cur & 3)) & 3);
                **d.borrow_mut() = u32::from_le_bytes(
                    buf.get(cur..cur + 4)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (u32)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                );
                cur += 4;
            }
            I32(d) => {
                cur = cur + ((4 - (cur & 3)) & 3);
                **d.borrow_mut() = i32::from_le_bytes(
                    buf.get(cur..cur + 4)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (i32)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                );
                cur += 4;
            }
            U16(d) => {
                cur = cur + (cur & 1);
                **d.borrow_mut() = u16::from_le_bytes(
                    buf.get(cur..cur + 2)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (u16)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                );
                cur += 2;
            }
            I16(d) => {
                cur = cur + (cur & 1);
                **d.borrow_mut() = i16::from_le_bytes(
                    buf.get(cur..cur + 2)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (i16)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                );
                cur += 2;
            }
            U8(d) => {
                **d.borrow_mut() = *buf.get(cur).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Property data overflow (u8)")
                })?;
                cur += 1;
            }
            I8(d) => {
                **d.borrow_mut() = *buf.get(cur).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Property data overflow (i8)")
                })? as i8;
                cur += 1;
            }
            Bool(d) => {
                let flipped = **d.borrow();
                **d.borrow_mut() = !flipped;
            }
            OU32(d) => {
                cur = cur + ((4 - (cur & 3)) & 3);
                **d.borrow_mut() = Some(u32::from_le_bytes(
                    buf.get(cur..cur + 4)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (u32)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                ));
                cur += 4;
            }
            OI32(d) => {
                cur = cur + ((4 - (cur & 3)) & 3);
                **d.borrow_mut() = Some(i32::from_le_bytes(
                    buf.get(cur..cur + 4)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (i32)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                ));
                cur += 4;
            }
            OU16(d) => {
                cur = cur + (cur & 1);
                **d.borrow_mut() = Some(u16::from_le_bytes(
                    buf.get(cur..cur + 2)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (u16)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                ));
                cur += 2;
            }
            OI16(d) => {
                cur = cur + (cur & 1);
                **d.borrow_mut() = Some(i16::from_le_bytes(
                    buf.get(cur..cur + 2)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidData,
                                "Property data overflow (i16)",
                            )
                        })?
                        .try_into()
                        .unwrap(),
                ));
                cur += 2;
            }
            OU8(d) => {
                **d.borrow_mut() = Some(*buf.get(cur).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Property data overflow (u8)")
                })?);
                cur += 1;
            }
            OI8(d) => {
                **d.borrow_mut() = Some(*buf.get(cur).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "Property data overflow (i8)")
                })? as i8);
                cur += 1;
            }
            Unused => {
                // Nothing to do
            }
            Flag(_) => {
                // To make the compiler happy, this is covered above
            }
        }
    }

    cur = cur + ((4 - (cur & 3)) & 3);
    Ok(buf.split_off(cur.min(buf.len())))
}

// CountOfBytesWithCompressionFlag parser
pub(crate) fn get_cob_string(cob: u32, buf: Option<&[u8]>) -> Option<(String, usize)> {
    let len_bytes = (cob & 0x7fffffff) as usize;
    let comp = (cob & 0x80000000) != 0;
    let buf = buf?.get(0..len_bytes)?;
    let s = if comp {
        utf8dec_rs::decode_win_str(buf, 1252)
    } else {
        utf8dec_rs::decode_utf16le_str(buf)
    };
    Some((s, len_bytes + ((4 - (len_bytes & 3)) & 3)))
}

pub(crate) fn get_array_string(buf: Option<&[u8]>) -> Option<(String, usize)> {
    let buf = buf?;
    let coc = u32::from_le_bytes(buf.get(0..4)?.try_into().unwrap());
    let cob: u32;
    if coc & 0x80000000 != 0 {
        cob = coc;
    } else {
        cob = (coc & !0x80000000) * 2;
        if cob & 0x80000000 != 0 {
            return None;
        }
    }
    let (s, slen) = get_cob_string(cob, buf.get(4..))?;
    Some((s, slen + 4))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_set_data_properties() -> Result<(), io::Error> {
        let mut tu8: Option<u8> = None;
        let mut tnu8: Option<u8> = None;
        let mut ti8: Option<i8> = None;
        let mut tni8: Option<u8> = None;
        let mut tu16: Option<u16> = None;
        let mut tnu16: Option<u16> = None;
        let mut ti16: Option<i16> = None;
        let mut tni16: Option<i16> = None;
        let mut tu32: Option<u32> = None;
        let mut tnu32: Option<u32> = Some(0xacab1337);
        let mut ti32: Option<i32> = None;
        let mut tni32: Option<i32> = Some(0x1337acab);
        let mut tunset: Option<()> = None;
        let mut tnunset: Option<()> = None;
        let mut tset: Option<()> = Some(());
        let mut tnset: Option<()> = Some(());
        let mut tfalse: bool = false;
        let mut tnfalse: bool = false;
        let mut ttrue: bool = true;
        let mut tntrue: bool = true;

        let buf: [u8; 26] = [
            24, 0, // size
            0b10101010, 0, 0b10010001, 0b01000011, // mask
            0xab, 0xac, // u16
            0xaa, 00, // u8 + padding
            0x37, 0x13, 0, 0, // i16 + padding
            0xe3, 0xff, 0xc0, 0, // i32
            42, 0, 0, 0, // i8 + padding
            0xbe, 0xba, 0xad, 0xde, // u32
        ];

        let mask: PropertyMask = [
            Unused,
            property_mask_bit!(tu16),
            Unused,
            property_mask_bit!(tu8),
            Unused,
            property_mask_bit!(ti16),
            Unused,
            property_mask_bit!(ti32),
            property_mask_bit!(tnu8),
            property_mask_bit!(tnu16),
            property_mask_bit!(tnu32),
            property_mask_bit!(tnunset),
            property_mask_bit!(tni8),
            property_mask_bit!(tni16),
            property_mask_bit!(tni32),
            Unused,
            property_mask_bit!(tunset),
            Unused,
            Unused,
            Unused,
            property_mask_bit!(tfalse),
            property_mask_bit!(tnfalse),
            Unused,
            property_mask_bit!(ti8),
            property_mask_bit!(tu32),
            property_mask_bit!(tset),
            property_mask_bit!(tnset),
            Unused,
            Unused,
            Unused,
            property_mask_bit!(ttrue),
            property_mask_bit!(tntrue),
        ];

        set_data_properties(&mut &buf[..], &mask)?;
        assert_eq!(tu8, Some(0xaa));
        assert_eq!(ti8, Some(42));
        assert_eq!(tu16, Some(0xacab));
        assert_eq!(ti16, Some(0x1337));
        assert_eq!(tu32, Some(0xdeadbabe));
        assert_eq!(ti32, Some(0xc0ffe3));
        assert_eq!(tunset, Some(()));
        assert_eq!(tnu8, None);
        assert_eq!(tni8, None);
        assert_eq!(tnu16, None);
        assert_eq!(tni16, None);
        assert_eq!(tnu32, Some(0xacab1337));
        assert_eq!(tni32, Some(0x1337acab));
        assert_eq!(tnunset, None);
        assert_eq!(tset, Some(()));
        assert_eq!(tnset, None);
        assert_eq!(tfalse, true);
        assert_eq!(tnfalse, false);
        assert_eq!(ttrue, false);
        assert_eq!(tntrue, true);

        Ok(())
    }
}
