//! Extractor for UDF images
//!
//! This module supports extraction of metadata and files from UDF images
//!
//! All legal block sizes are supported and automatically detected
//!
//! Files using allocation descriptor continuation are currently NOT supported

#![warn(missing_docs)]

pub mod ecma167;

use ecma167::*;
use std::collections::HashSet;
use std::io::{Read, Seek};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

/// UDF extractor
pub struct Udf<'r, R: Read + Seek> {
    r: &'r mut R,
    /// Sector size
    pub ss: u64,
    /// Volume Descriptor Sequence
    pub vds: VolumeDescriptorSequence,
    /// Missing Terminating Extended Area Descriptor
    pub missing_tea: bool,
}

impl<'r, R: Read + Seek> Udf<'r, R> {
    /// Creates a new UDF extractor
    #[instrument(skip_all)]
    pub fn new(r: &'r mut R) -> Result<Self, std::io::Error> {
        // Scan descriptors
        let mut has_bea = false;
        let mut has_tea = false;
        let mut has_nsr = false;
        let mut descriptor = [0u8; 6];
        for i in 16..32 {
            r.seek(std::io::SeekFrom::Start(i * 2048))?;
            r.read_exact(&mut descriptor)?;
            match &descriptor {
                b"\0BEA01" => {
                    debug!("Beginning Extended Area Descriptor at sector {i}");
                    if has_bea {
                        warn!("Multiple Beginning Extended Area Descriptors");
                    } else {
                        has_bea = true;
                    }
                }
                b"\0NSR02" | b"\0NSR03" => {
                    debug!("NSR0{} Descriptor at sector {i}", descriptor[5] - 0x30);
                    if has_nsr {
                        warn!("Multiple NSR Descriptors");
                    } else {
                        has_nsr = true;
                    }
                }
                b"\0TEA01" => {
                    debug!("TEA01 Descriptor at sector {i}");
                    has_tea = true;
                    break;
                }
                _ => {}
            }
        }
        if !has_bea || !has_nsr {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "UDF descriptors not found - not an UDF image",
            ));
        }
        if !has_tea {
            warn!("Terminating Extended Area Descriptor not found");
        }

        // Guess blocksize and find the Anchor Volume Descriptor Pointer
        let imgsize = r.seek(std::io::SeekFrom::End(0))?;
        let mut ss = 256u64;
        let anchor = loop {
            // Range is 512-32768
            ss <<= 1;
            if ss > 32768 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Anchor Volume Descriptor Pointer not found",
                ));
            }

            // 1. try block 256
            r.seek(std::io::SeekFrom::Start(ss * 256))?;
            if let Ok(anchor) = AnchorVolumeDescriptorPointer::new(r, 256) {
                break anchor;
            }

            // 2. try last_block - 256
            let last_block = match (imgsize / ss)
                .checked_sub(1)
                .and_then(|v| u32::try_from(v).ok())
            {
                Some(v) => v,
                _ => continue,
            };
            if last_block > 256 {
                r.seek(std::io::SeekFrom::Start(ss * (u64::from(last_block) - 256)))?;
                if let Ok(anchor) = AnchorVolumeDescriptorPointer::new(r, last_block - 256) {
                    break anchor;
                }
            }

            // 3. try last_block
            r.seek(std::io::SeekFrom::Start(ss * u64::from(last_block)))?;
            if let Ok(anchor) = AnchorVolumeDescriptorPointer::new(r, last_block) {
                break anchor;
            }

            // 4. try 512
            r.seek(std::io::SeekFrom::Start(ss * 512))?;
            if let Ok(anchor) = AnchorVolumeDescriptorPointer::new(r, 512) {
                break anchor;
            }
        };
        debug!("Sector size: {ss}");
        debug!("Anchor Volume Descriptor Pointer: {anchor:?}");

        // Read Volume Descriptor Sequence
        let mut vds = VolumeDescriptorSequence::new(r, &anchor.main, ss)?;

        // Update partition maps for each lvd, this can only happen now after the whole sequence is parsed
        for lvd in vds.lvds.iter_mut() {
            for pmap in lvd.partition_maps.iter_mut().flatten() {
                pmap.set_partition_index(vds.pds.as_slice());
            }
        }
        debug!("VDS: {vds:#?}");
        Ok(Self {
            r,
            ss,
            vds,
            missing_tea: !has_tea,
        })
    }

    /// Opens an image volume for extraction
    #[instrument(skip(self))]
    pub fn open_volume<'u>(
        &'u mut self,
        nvol: usize,
    ) -> Option<Result<VolumeReader<'r, 'u, R>, std::io::Error>> {
        if nvol >= self.vds.lvds.len() {
            None
        } else {
            Some(VolumeReader::new(self, nvol))
        }
    }
}

/// A Volume Extractor
pub struct VolumeReader<'r, 'u, R: Read + Seek> {
    udf: &'u mut Udf<'r, R>,
    volume_index: usize,
    files: Vec<(String, FileEntry)>,
    /// Filesystem contains one or more block devices
    pub has_blockdev: bool,
    /// Filesystem contains one or more character devices
    pub has_chardev: bool,
    /// Filesystem contains one or more pipes
    pub has_fifo: bool,
    /// Filesystem contains one or more symbolic links
    pub has_symlink: bool,
    /// Filesystem contains one or more hard links
    pub has_hardlink: bool,
    /// Filesystem contains one or more file entries of unknown type
    pub has_unknown: bool,
}

impl<'r, 'u, R: Read + Seek> VolumeReader<'r, 'u, R> {
    #[instrument(skip(udf))]
    fn new(udf: &'u mut Udf<'r, R>, volume_index: usize) -> Result<Self, std::io::Error> {
        let ss = udf.ss;
        let lvd = udf
            .vds
            .lvds
            .get(volume_index)
            .expect("Internal error index out of bounds");
        if !lvd.desc_charset.is_osta_cs0() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid Logical Volume Descriptor charset",
            ));
        }
        if u64::from(lvd.block_size) != ss {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Logical Volume Descriptor block size doesn't match the one from the image",
            ));
        }
        if !lvd.domain_identifier.is_osta_udf_compliant() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Logical Volume Descriptor with invalid Domain Identifier",
            ));
        }
        debug!("Processing volume {volume_index}\n{lvd:?}");
        let mut res = Self {
            udf,
            volume_index,
            files: Vec::new(),
            has_blockdev: false,
            has_chardev: false,
            has_fifo: false,
            has_symlink: false,
            has_hardlink: false,
            has_unknown: false,
        };

        if res.lvd().root_desc.length < 512 {
            // In UDF 2.60 there is a single File Set Descriptor in the sequence
            // and the File Set Descriptor size is limited to 512 bytes
            //
            // This is usually set to the sector size
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Insufficient File Set Descriptor length",
            ));
        }
        let root_fsd_lba = res.to_absolute_lba(&res.lvd().root_desc).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Cannot locate the Root File Set Descriptor inside the volume",
            )
        })?;
        res.udf
            .r
            .seek(std::io::SeekFrom::Start(u64::from(root_fsd_lba) * ss))?;
        let fsd = FileSetDescriptor::new(
            &mut res.udf.r,
            res.udf.vds.lvds.get(volume_index).unwrap().root_desc.lba,
        )?;
        debug!("Root File Set: {fsd:?}");

        if fsd.root_dir_icb.length < 512 {
            // This is usually set to the sector size
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Insufficient Root File Entry length",
            ));
        }
        let root_dir_lba = res.to_absolute_lba(&fsd.root_dir_icb).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Cannot locate the Root File Entry inside the volume",
            )
        })?;
        res.udf
            .r
            .seek(std::io::SeekFrom::Start(u64::from(root_dir_lba) * ss))?;
        let root = FileEntry::new(&mut res.udf.r, &fsd.root_dir_icb, ss)?;
        debug!("Root: {root:?}");

        let mut seen = HashSet::<u32>::new();
        let mut dirs = vec![("".to_string(), root)];
        while let Some((path, dir)) = dirs.pop() {
            debug!("Parsing directory \"{path}\"\n{dir:?}");
            let mut fids: Vec<FileIdentifierDescriptor> = Vec::new();
            let mut rdr = EntryReader::new(&mut res, dir);
            while !rdr.feof() {
                let fid = FileIdentifierDescriptor::new(&mut rdr, ss)?;
                if fid.is_parent() {
                    // ..
                    continue;
                }
                if fid.is_deleted() {
                    debug!("Entry \"{}\" is deleted", fid.identifier);
                    continue;
                }
                fids.push(fid);
            }
            for fid in fids {
                let full_path = format!("{path}/{}", fid.identifier);
                if let Some(fid_lba) = res.to_absolute_lba(&fid.icb) {
                    res.udf
                        .r
                        .seek(std::io::SeekFrom::Start(u64::from(fid_lba) * ss))?;
                    if !seen.insert(fid_lba) {
                        // FIXME: we could collect all names for each entry but
                        // 1. this is easily done for file links
                        // 2. directory links require more care in order not to infloop
                        debug!("Skipping hard link \"{}\"", full_path);
                        res.has_hardlink = true;
                        continue;
                    }
                    let fe = FileEntry::new(&mut res.udf.r, &fid.icb, ss)?;
                    // FIXME: change warns to debugs after research
                    if fid.is_directory() {
                        if !fe.icb_tag.is_directory() {
                            warn!("Directory fid with non directory fe \"{full_path}\"");
                        }
                        dirs.push((full_path, fe));
                    } else if fe.icb_tag.is_regular() {
                        if fe.icb_tag.is_directory() {
                            warn!("File fid with non directory fe \"{full_path}\"");
                        }
                        res.files.push((full_path, fe));
                    } else if fe.icb_tag.is_block() {
                        warn!("Skipping block device \"{}\"", full_path);
                        res.has_blockdev = true;
                    } else if fe.icb_tag.is_char() {
                        warn!("Skipping character device \"{}\"", full_path);
                        res.has_chardev = true;
                    } else if fe.icb_tag.is_fifo() {
                        warn!("Skipping fifo \"{}\"", full_path);
                        res.has_fifo = true;
                    } else if fe.icb_tag.is_link() {
                        warn!("Skipping symbolic link \"{}\"", full_path);
                        res.has_symlink = true;
                    } else {
                        warn!(
                            "Skipping unknown entry \"{}\" ({:?})",
                            full_path, fe.icb_tag
                        );
                        res.has_unknown = true;
                    }
                } else {
                    warn!("Cannot locate entry \"{}\" inside the volume", full_path);
                }
            }
        }
        Ok(res)
    }

    /// The Logical Volume Descriptor
    pub fn lvd(&self) -> &LogicalVolumeDescriptor {
        self.udf
            .vds
            .lvds
            .get(self.volume_index)
            .expect("Internal error: index out of bounds")
    }

    fn to_absolute_lba(&self, long_ad: &LongAD) -> Option<u32> {
        let index = self
            .lvd()
            .partition_maps
            .get(usize::from(long_ad.part_num))?
            .as_ref()
            .map(|pm| pm.partition_index)??;
        self.udf
            .vds
            .pds
            .get(index)
            .map(|p| p.partition_starting_location + long_ad.lba)
    }

    /// Returns the name and a reader for the specified file
    #[instrument(skip(self))]
    pub fn open_file<'v>(
        &'v mut self,
        nfile: usize,
    ) -> Option<(String, EntryReader<'r, 'u, 'v, R>)> {
        let (name, entry) = self.files.get(nfile)?;
        Some((name.to_string(), EntryReader::new(self, entry.clone())))
    }
}

/// A [`FileEntry`] reader
pub struct EntryReader<'r, 'u, 'v, R: Read + Seek> {
    vol: &'v mut VolumeReader<'r, 'u, R>,
    /// The File Entry this reader refers to
    pub entry: FileEntry,
    offset: u64,
}

impl<'r, 'u, 'v, R: Read + Seek> EntryReader<'r, 'u, 'v, R> {
    fn new(vol: &'v mut VolumeReader<'r, 'u, R>, entry: FileEntry) -> Self {
        Self {
            vol,
            entry,
            offset: 0,
        }
    }

    fn feof(&self) -> bool {
        self.offset == self.entry.information_length
    }
}

impl<'r, 'u, 'v, R: Read + Seek> Read for EntryReader<'r, 'u, 'v, R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        if self.offset >= self.entry.information_length {
            return Ok(0);
        }
        match &self.entry.data_location {
            FileDataLocation::UseADs(ads) => {
                let mut offset_in_ad = self.offset;
                for ad in ads {
                    let ad_len = u64::from(ad.ad_unmasked_length());
                    if offset_in_ad >= ad_len {
                        offset_in_ad -= ad_len;
                        continue;
                    }
                    let mut len = usize::try_from(ad_len - offset_in_ad)
                        .unwrap_or(usize::MAX)
                        .min(buf.len());
                    if ad.ad_is_recorded() {
                        let lba = self.vol.to_absolute_lba(ad).ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "LongAd offset error",
                            )
                        })?;
                        self.vol.udf.r.seek(std::io::SeekFrom::Start(
                            u64::from(lba) * self.vol.udf.ss + offset_in_ad,
                        ))?;
                        len = self.vol.udf.r.read(&mut buf[0..len])?;
                    } else {
                        buf[0..len].fill(0);
                    }
                    self.offset += len as u64;
                    return Ok(len);
                }
                unreachable!("Internal error: reached the end of ADs");
            }
            FileDataLocation::Embedded(data) => {
                /* Note:
                 * self.entry.information_length is guaranteed to fall in the usize range in FileEntry::new
                 * self.offset is guaranteed to also be in the usize range but the initial check here
                 * Therefore casting to usize is safe
                 */
                let start = self.offset as usize;
                let len = buf
                    .len()
                    .min((self.entry.information_length as usize) - start);
                buf[0..len].copy_from_slice(&data[start..(start + len)]);
                self.offset += len as u64;
                Ok(len)
            }
        }
    }
}

impl<'r, 'u, 'v, R: Read + Seek> Seek for EntryReader<'r, 'u, 'v, R> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> Result<u64, std::io::Error> {
        match pos {
            std::io::SeekFrom::Start(whence) => self.offset = whence,
            std::io::SeekFrom::Current(whence) => {
                if whence < 0 {
                    if self.offset < (-whence as u64) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Attempted to seek before the start of the stream",
                        ));
                    }
                    self.offset -= -whence as u64;
                } else {
                    self.offset += whence as u64;
                }
            }
            std::io::SeekFrom::End(whence) => {
                if whence < 0 {
                    if self.entry.information_length < (-whence as u64) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "Attempted to seek before the start of the stream",
                        ));
                    }
                    self.offset = self.entry.information_length - (-whence as u64);
                } else {
                    self.offset = self.entry.information_length + (whence as u64);
                }
            }
        }
        Ok(self.offset)
    }

    fn stream_position(&mut self) -> Result<u64, std::io::Error> {
        Ok(self.offset)
    }
}
