mod config;

use backend_utils::objects::*;
use std::collections::HashSet;
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let config = config::Config::new()?;
    backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
        process_request(request, &config)
    })?;
    unreachable!()
}

#[derive(serde::Serialize)]
struct UdfPV {
    /// Volume Descriptor Sequence Number
    seq: u32,
    /// Primary Volume Descriptor Number
    number: u32,
    /// Volume Identifier
    identifier: String,
    /// Volume Sequence Number
    sequence_number: u16,
    /// Maximum Volume Sequence Number
    max_sequence_number: u16,
    /// Interchange Level
    interchange_level: u16,
    /// Maximum Interchange Level
    max_interchange_level: u16,
    /// Volume Set Identifier
    set_identifier: String,
    /// Volume Abstract
    has_abstract: bool,
    /// Volume Copyright Notice
    has_copyright: bool,
    /// Application Identifier
    app_identifier: String,
    /// Recording Date and Time
    datetime: Option<String>,
    /// Recording Date and Time
    timestamp: Option<i64>,
    /// Predecessor Volume Descriptor Sequence Location
    predecessor_seq_location: u32,
}

impl From<&cdfs::udf::ecma167::PrimaryVolumeDescriptor> for UdfPV {
    fn from(pvd: &cdfs::udf::ecma167::PrimaryVolumeDescriptor) -> Self {
        Self {
            seq: pvd.desc_sequence_number,
            number: pvd.number,
            identifier: pvd.identifier.to_string(),
            sequence_number: pvd.sequence_number,
            max_sequence_number: pvd.max_sequence_number,
            interchange_level: pvd.interchange_level,
            max_interchange_level: pvd.max_interchange_level,
            set_identifier: pvd.set_identifier.to_string(),
            has_abstract: pvd.vol_abstract.length != 0,
            has_copyright: pvd.copyright_notice.length != 0,
            app_identifier: pvd.app_identifier.lossy_identifier(),
            datetime: pvd.datetime.to_string_maybe(),
            timestamp: pvd.datetime.to_ts_maybe(),
            predecessor_seq_location: pvd.predecessor_seq_location,
        }
    }
}

#[derive(serde::Serialize)]
struct UdfPartition {
    /// Volume Descriptor Sequence Number
    seq: u32,
    /// Whether the area is allocated
    alloc: bool,
    /// Partition Number
    number: u16,
    /// Partition Contents
    contents: String,
    /// Access Type
    access: String,
    /// Partition Starting Location
    starting_location: u32,
    /// Partition Length
    length: u32,
    /// Implementation Identifier
    impl_identifier: String,
}

impl From<&cdfs::udf::ecma167::PartitionDescriptor> for UdfPartition {
    fn from(pd: &cdfs::udf::ecma167::PartitionDescriptor) -> Self {
        Self {
            seq: pd.desc_sequence_number,
            alloc: pd.flags & 1 != 0,
            number: pd.partition_number,
            contents: pd.partition_contents.lossy_identifier(),
            access: match pd.access_type {
                0 => "PseudoOverwriteable".to_string(),
                1 => "ReadOnly".to_string(),
                2 => "WriteOnce".to_string(),
                3 => "Rewriteable".to_string(),
                4 => "Overwriteable".to_string(),
                v => format!("Invalid({v})"),
            },
            starting_location: pd.partition_starting_location,
            length: pd.partition_length,
            impl_identifier: pd.impl_identifier.lossy_identifier(),
        }
    }
}

#[derive(serde::Serialize)]
struct UdfIUV {
    /// Volume Descriptor Sequence Number
    seq: u32,
    /// Logical Volume Identifier
    id: String,
    /// Owner Name
    owner: String,
    /// Organization Name
    org: String,
    /// Contact Information
    contact: String,
    /// ImplementationID (UDF 2.2.7.2.4)
    impl_identifier: String,
}

impl From<&cdfs::udf::ecma167::ImplementationUseVolumeDescriptor> for UdfIUV {
    fn from(iuvd: &cdfs::udf::ecma167::ImplementationUseVolumeDescriptor) -> Self {
        Self {
            seq: iuvd.desc_sequence_number,
            id: iuvd.lv_identifier.to_string(),
            owner: iuvd.lv_info1.to_string(),
            org: iuvd.lv_info2.to_string(),
            contact: iuvd.lv_info3.to_string(),
            impl_identifier: iuvd.impl_identifier.lossy_identifier(),
        }
    }
}

#[derive(serde::Serialize)]
struct UdfLV {
    /// Volume Descriptor Sequence Number
    seq: u32,
    /// Logical Volume Identifier
    identifier: String,
    /// Implementation Identifier
    impl_identifier: String,
    /// Number of Partition Maps
    num_partition_maps: usize,
}

impl From<&cdfs::udf::ecma167::LogicalVolumeDescriptor> for UdfLV {
    fn from(lvd: &cdfs::udf::ecma167::LogicalVolumeDescriptor) -> Self {
        Self {
            seq: lvd.desc_sequence_number,
            identifier: lvd.identifier.to_string(),
            impl_identifier: lvd.impl_identifier.lossy_identifier(),
            num_partition_maps: lvd.partition_maps.len(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct IsoVolume {
    /// Volume Descriptor Type
    desc_type: String,
    /// Volume Descriptor Version
    version: u8,
    /// Volume Flags
    flags: u8,
    /// System Identifier
    system: String,
    /// Volume Identifier
    id: String,
    /// Volume Space Size
    space_size: u32,
    /// Volume Set Size
    set_size: u16,
    /// Volume Sequence Number
    seq: u16,
    /// Logical Block Size
    block_size: u16,
    /// Volume Set Identifier
    set: String,
    /// Publisher Identifier
    publisher: String,
    /// Data Preparer Identifier
    preparer: String,
    /// Application Identifier
    application: String,
    /// Copyright File Identifier
    copyright_file: String,
    /// Abstract File Identifier
    abstract_file: String,
    /// Bibliographic File Identifier
    bibliographic_file: String,
    /// Volume Creation Date and Time
    creation_dt: String,
    /// Volume Modification Date and Time
    modification_dt: String,
    /// Volume Expiration Date and Time
    expiration_dt: String,
    /// Volume Effective Date and Time
    effective_dt: String,
    /// File Structure Version
    fs_ver: u8,
    /// Whether this volume conforms to the joliet specifications
    joliet: bool,
}

impl From<&cdfs::iso::Volume> for IsoVolume {
    fn from(v: &cdfs::iso::Volume) -> Self {
        let desc_type = match v.descriptor_type {
            0 => "Boot".to_string(),
            1 => "Primary".to_string(),
            2 => "Enahanced".to_string(),
            3 => "Partition".to_string(),
            v => format!("Unknown({v})"),
        };
        Self {
            desc_type,
            version: v.version,
            flags: v.flags,
            system: v.system_id.clone(),
            id: v.volume_id.clone(),
            space_size: v.volume_space_size,
            set_size: v.volume_set_size,
            seq: v.volume_sequence_number,
            block_size: v.block_size,
            set: v.volume_set_id.clone(),
            publisher: v.publisher_id.clone(),
            preparer: v.preparer_id.clone(),
            application: v.application_id.clone(),
            copyright_file: v.copyright_file_id.clone(),
            abstract_file: v.abstract_file_id.clone(),
            bibliographic_file: v.bibliographic_file_id.clone(),
            creation_dt: v.volume_creation_dt.to_string(),
            modification_dt: v.volume_modification_dt.to_string(),
            expiration_dt: v.volume_expiration_dt.to_string(),
            effective_dt: v.volume_effective_dt.to_string(),
            fs_ver: v.file_structure_version,
            joliet: v.is_joliet,
        }
    }
}

fn to_iso_lvl1(path: &str) -> String {
    let mut path_parts: Vec<String> = path.split('/').map(|s| s.to_ascii_uppercase()).collect();
    if let Some(basename) = path_parts.pop() {
        for p in path_parts.iter_mut() {
            (*p).truncate(8);
            (*p) = (*p).replace(|c: char| !c.is_ascii_alphanumeric(), "_");
        }
        let mut name_parts = basename.splitn(3, '.').take(2);
        if let Some(name) = name_parts.next() {
            let mut name = name.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
            name.truncate(8);
            if let Some(ext) = name_parts.next() {
                let mut ext = ext.replace(|c: char| !c.is_ascii_alphanumeric(), "_");
                ext.truncate(3);
                path_parts.push(format!("{}.{}", name, ext));
            } else {
                path_parts.push(name);
            }
        } else {
            path_parts.push("".to_string());
        }
    }
    path_parts.join("/")
}

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    let mut udf_entries: HashSet<(String, u64)> = HashSet::new();
    info!("Parsing {}", input_name.display());
    let mut f = std::fs::File::open(input_name)?;
    let mut metadata = Metadata::new();
    let mut symbols: Vec<String> = Vec::new();
    let mut parsed = false;
    let mut children: Vec<BackendResultChild> = Vec::new();
    let mut limits_reached = false;
    let mut processed_size = 0u64;

    // Extract as UDF first
    match cdfs::udf::Udf::new(&mut f) {
        Ok(mut udf) => {
            parsed = true;
            symbols.push("UDF".to_string());
            let mut udf_meta = Metadata::new();
            udf_meta.insert("sector_size".to_string(), udf.ss.into());
            let vds = &udf.vds;
            udf_meta.insert("num_pvds".to_string(), vds.pvds.len().into());
            let mut items: Vec<UdfPV> = Vec::new();
            for pvd in vds.pvds.iter().take(4) {
                items.push(pvd.into());
            }
            udf_meta.insert("pvds".to_string(), serde_json::to_value(items).unwrap());

            udf_meta.insert("num_pds".to_string(), vds.pds.len().into());
            let mut items: Vec<UdfPartition> = Vec::new();
            for pd in vds.pds.iter().take(4) {
                items.push(pd.into());
            }
            udf_meta.insert("pds".to_string(), serde_json::to_value(items).unwrap());

            udf_meta.insert("num_iuvds".to_string(), vds.iuvds.len().into());
            let mut items: Vec<UdfIUV> = Vec::new();
            for iuvd in vds.iuvds.iter().take(4) {
                items.push(iuvd.into());
            }
            udf_meta.insert("iuvds".to_string(), serde_json::to_value(items).unwrap());

            udf_meta.insert("num_lvds".to_string(), vds.lvds.len().into());
            let mut items: Vec<UdfLV> = Vec::new();
            for lvd in vds.lvds.iter().take(4) {
                items.push(lvd.into());
            }
            udf_meta.insert("lvds".to_string(), serde_json::to_value(items).unwrap());
            let mut has_blockdev = false;
            let mut has_chardev = false;
            let mut has_fifo = false;
            let mut has_symlink = false;
            let mut has_hardlink = false;
            let mut has_unknown = false;
            let mut nvol = 0usize;
            'outer: while let Some(maybe_vol) = udf.open_volume(nvol) {
                match maybe_vol {
                    Ok(mut volume) => {
                        let volname = volume.lvd().identifier.to_string();
                        info!("Opened UDF volume \"{}\"\n{:?}", volname, volume.lvd());
                        let mut nfile = 0usize;
                        has_blockdev |= volume.has_blockdev;
                        has_chardev |= volume.has_chardev;
                        has_fifo |= volume.has_fifo;
                        has_symlink |= volume.has_symlink;
                        has_hardlink |= volume.has_hardlink;
                        has_unknown |= volume.has_unknown;
                        while let Some((fname, mut r)) = volume.open_file(nfile) {
                            if children.len() >= config.max_children {
                                debug!("Max children reached, breaking out");
                                limits_reached = true;
                                break 'outer;
                            }
                            if processed_size > config.max_processed_size {
                                debug!("Max processed size reached, breaking out");
                                limits_reached = true;
                                break 'outer;
                            }
                            let mut file_syms: Vec<String> = Vec::new();
                            let path = if r.entry.information_length >= config.max_child_output_size
                            {
                                info!(
                                    "Skipping UDF file \"{}\" ({} bytes)",
                                    fname, r.entry.information_length
                                );
                                limits_reached = true;
                                file_syms.push("TOOBIG".to_string());
                                None
                            } else {
                                info!("Extracting UDF file \"{}\"\n{:?}", fname, r.entry);
                                let mut output_file =
                                    tempfile::NamedTempFile::new_in(&config.output_path)?;
                                match std::io::copy(&mut r, &mut output_file) {
                                    Ok(len) if len == r.entry.information_length => {
                                        udf_entries.insert((to_iso_lvl1(&fname), len));
                                        processed_size += len;
                                        Some(
                                            output_file
                                                .into_temp_path()
                                                .keep()
                                                .unwrap()
                                                .into_os_string()
                                                .into_string()
                                                .unwrap(),
                                        )
                                    }
                                    Ok(len) => {
                                        processed_size += len;
                                        warn!(
                                            "UDF file {} is incomplete {} / {}",
                                            fname, len, r.entry.information_length
                                        );
                                        file_syms.push("CORRUPTED".to_string());
                                        file_syms.push("TRUNCATED".to_string());
                                        None
                                    }
                                    Err(e)
                                        if [
                                            std::io::ErrorKind::InvalidData,
                                            std::io::ErrorKind::UnexpectedEof,
                                        ]
                                        .contains(&e.kind()) =>
                                    {
                                        warn!("UDF file {} is corrupted: {}", fname, e);
                                        file_syms.push("CORRUPTED".to_string());
                                        None
                                    }
                                    Err(e) => {
                                        error!("Error extracting UDF file {}: {}", fname, e);
                                        return Err(e);
                                    }
                                }
                            };
                            let mut file_meta = Metadata::new();
                            file_meta.insert("udf_vol".to_string(), nvol.into());
                            file_meta.insert("ord".to_string(), nfile.into());
                            file_meta.insert("name".to_string(), fname.into());
                            file_meta.insert("uid".to_string(), r.entry.uid.into());
                            file_meta.insert("gid".to_string(), r.entry.gid.into());
                            file_meta.insert("perms".to_string(), r.entry.permissions.into());
                            file_meta.insert("perms_str".to_string(), r.entry.perms_str().into());
                            file_meta.insert("links".to_string(), r.entry.file_link_count.into());
                            file_meta.insert(
                                "atime".to_string(),
                                r.entry.access_time.to_string_maybe().into(),
                            );
                            file_meta.insert(
                                "mtime".to_string(),
                                r.entry.modification_time.to_string_maybe().into(),
                            );
                            file_meta.insert(
                                "ctime".to_string(),
                                r.entry.creation_time.to_string_maybe().into(),
                            );
                            file_meta.insert(
                                "xtime".to_string(),
                                r.entry.creation_time.to_string_maybe().into(),
                            );
                            file_meta.insert("chkpt".to_string(), r.entry.checkpoint.into());
                            file_meta.insert(
                                "impl".to_string(),
                                r.entry.implementation_identifier.lossy_identifier().into(),
                            );
                            file_meta.insert("unique_id".to_string(), r.entry.unique_id.into());
                            file_meta.insert("extended".to_string(), r.entry.is_extended().into());
                            file_meta.insert("embedded".to_string(), r.entry.is_embedded().into());
                            children.push(BackendResultChild {
                                path,
                                force_type: None,
                                symbols: file_syms,
                                relation_metadata: file_meta,
                            });
                            nfile += 1;
                        }
                    }
                    Err(e) => eprintln!("Error retrieving volume {nvol}: {e}"),
                }
                nvol += 1;
            }
            udf_meta.insert("no_tea".to_string(), udf.missing_tea.into());
            udf_meta.insert("has_blk".to_string(), has_blockdev.into());
            udf_meta.insert("has_chardev".to_string(), has_chardev.into());
            udf_meta.insert("has_fifo".to_string(), has_fifo.into());
            udf_meta.insert("has_symlink".to_string(), has_symlink.into());
            udf_meta.insert("has_hardlink".to_string(), has_hardlink.into());
            udf_meta.insert("has_unknown".to_string(), has_unknown.into());
            metadata.insert(
                "udf".to_string(),
                serde_json::value::Value::Object(udf_meta),
            );
        }
        Err(e)
            if [
                std::io::ErrorKind::InvalidData,
                std::io::ErrorKind::UnexpectedEof,
            ]
            .contains(&e.kind()) =>
        {
            debug!("UDF open failed: {e}")
        }
        Err(e) => {
            error!("Error processing {}", request.object.object_id);
            return Err(e);
        }
    }

    // Extract as iso9660
    match cdfs::iso::Iso9660::new(&mut f) {
        Ok(mut iso) => {
            parsed = true;
            symbols.push("ISO9660".to_string());
            let mut iso_meta = Metadata::new();
            iso_meta.insert("num_vols".to_string(), iso.volumes.len().into());
            iso_meta.insert("offset".to_string(), iso.image_header_size.into());
            iso_meta.insert("sector_size".to_string(), iso.raw_sector_size.into());
            iso_meta.insert("bootable".to_string(), iso.is_bootable.into());
            iso_meta.insert("partitioned".to_string(), iso.is_partitioned.into());
            let mut vols: Vec<IsoVolume> = Vec::new();
            let mut nvol = 0usize;
            let mut has_loops = false;
            let mut bad_chains = false;
            'outer: while let Some(maybe_vol) = iso.open_volume(nvol) {
                match maybe_vol {
                    Ok(mut volrdr) => {
                        info!(
                            "Opened ISO volume \"{}\"\n{:?}",
                            volrdr.volume.volume_id, volrdr.volume
                        );
                        vols.push(IsoVolume::from(&volrdr.volume));
                        has_loops |= volrdr.has_loops;
                        bad_chains |= volrdr.bad_chains;
                        let mut nfile = 0usize;
                        while let Some((dr, mut r)) = volrdr.open_file(nfile) {
                            if children.len() >= config.max_children {
                                debug!("Max children reached, breaking out");
                                limits_reached = true;
                                break 'outer;
                            }
                            if processed_size > config.max_processed_size {
                                debug!("Max processed size reached, breaking out");
                                limits_reached = true;
                                break 'outer;
                            }
                            let mut file_syms: Vec<String> = Vec::new();
                            let path = if u64::from(dr.data_length) >= config.max_child_output_size
                            {
                                info!(
                                    "Skipping ISO file \"{}\" ({} bytes)",
                                    dr.file_id, dr.data_length
                                );
                                limits_reached = true;
                                file_syms.push("TOOBIG".to_string());
                                None
                            } else {
                                if udf_entries.contains(&(
                                    to_iso_lvl1(&dr.file_id),
                                    u64::from(dr.data_length),
                                )) {
                                    info!(
                                        "Skipping ISO file \"{}\" (already processed)",
                                        dr.file_id
                                    );
                                    nfile += 1;
                                    continue;
                                }
                                info!("Extracting ISO file \"{}\"\n{:?}", dr.file_id, dr);
                                udf_entries
                                    .insert((to_iso_lvl1(&dr.file_id), u64::from(dr.data_length)));
                                let mut output_file =
                                    tempfile::NamedTempFile::new_in(&config.output_path)?;
                                match std::io::copy(&mut r, &mut output_file) {
                                    Ok(len) if len == u64::from(dr.data_length) => {
                                        processed_size += len;
                                        Some(
                                            output_file
                                                .into_temp_path()
                                                .keep()
                                                .unwrap()
                                                .into_os_string()
                                                .into_string()
                                                .unwrap(),
                                        )
                                    }
                                    Ok(len) => {
                                        processed_size += len;
                                        warn!(
                                            "ISO file {} is incomplete {} / {}",
                                            dr.file_id, len, dr.data_length
                                        );
                                        file_syms.push("CORRUPTED".to_string());
                                        file_syms.push("TRUNCATED".to_string());
                                        None
                                    }
                                    Err(e)
                                        if [
                                            std::io::ErrorKind::InvalidData,
                                            std::io::ErrorKind::UnexpectedEof,
                                        ]
                                        .contains(&e.kind()) =>
                                    {
                                        warn!("ISO file {} is corrupted: {}", dr.file_id, e);
                                        file_syms.push("CORRUPTED".to_string());
                                        None
                                    }
                                    Err(e) => {
                                        error!("Error extracting UDF file {}: {}", dr.file_id, e);
                                        return Err(e);
                                    }
                                }
                            };
                            let mut file_meta = Metadata::new();
                            file_meta.insert("iso_vol".to_string(), nvol.into());
                            file_meta.insert("ord".to_string(), nfile.into());
                            file_meta.insert("interleaved".to_string(), dr.is_interleaved().into());
                            file_meta.insert("t".to_string(), dr.recording_dt.to_string().into());
                            file_meta.insert("name".to_string(), dr.file_id.into());
                            children.push(BackendResultChild {
                                path,
                                force_type: None,
                                symbols: file_syms,
                                relation_metadata: file_meta,
                            });
                            nfile += 1;
                        }
                    }
                    Err(e) => eprintln!("Error retrieving volume {nvol}: {e}"),
                }
                nvol += 1;
            }
            iso_meta.insert("volumes".to_string(), serde_json::json!(vols));
            iso_meta.insert("has_loops".to_string(), has_loops.into());
            iso_meta.insert("bad_chains".to_string(), bad_chains.into());
            metadata.insert(
                "iso9660".to_string(),
                serde_json::value::Value::Object(iso_meta),
            );
        }
        Err(e)
            if [
                std::io::ErrorKind::InvalidData,
                std::io::ErrorKind::UnexpectedEof,
            ]
            .contains(&e.kind()) =>
        {
            debug!("ISO9660 open failed: {e}")
        }
        Err(e) => {
            error!("Error processing {}", request.object.object_id);
            return Err(e);
        }
    }

    if limits_reached {
        symbols.push("LIMITS_REACHED".to_string());
    }

    if parsed {
        Ok(BackendResultKind::ok(BackendResultOk {
            symbols,
            object_metadata: metadata,
            children,
        }))
    } else {
        Ok(BackendResultKind::error(
            "Not an UDF/ISO9660 image".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::to_iso_lvl1;
    #[test]
    fn isol1() {
        assert_eq!(to_iso_lvl1("/"), "/");
        assert_eq!(to_iso_lvl1("/name"), "/NAME");
        assert_eq!(to_iso_lvl1("/nAme.eXt"), "/NAME.EXT");
        assert_eq!(to_iso_lvl1("/dir1/DIR2/nAme.eXt"), "/DIR1/DIR2/NAME.EXT");
        assert_eq!(to_iso_lvl1("/verylongname.verylongext"), "/VERYLONG.VER");
        assert_eq!(to_iso_lvl1("/spc dir/!nv@l1D.$%&"), "/SPC_DIR/_NV_L1D.___");
    }
}
