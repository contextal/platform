mod config;

use backend_utils::objects::*;
use macho_rs::MachO;
use std::path::PathBuf;
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::prelude::*;

#[instrument(level="error", skip_all, fields(object_id = request.object.object_id))]
fn process_request(
    request: &BackendRequest,
    config: &config::Config,
) -> Result<BackendResultKind, std::io::Error> {
    let input_name: PathBuf = [&config.objects_path, &request.object.object_id]
        .into_iter()
        .collect();
    info!("Parsing {}", input_name.display());
    let input_file = std::fs::File::open(input_name)?;
    let children: Vec<BackendResultChild> = Vec::new();
    match MachO::new(input_file) {
        Ok(p) => Ok(BackendResultKind::ok(BackendResultOk {
            symbols: Vec::<String>::new(),
            object_metadata: match serde_json::to_value(p).unwrap() {
                serde_json::Value::Object(v) => v,
                _ => unreachable!(),
            },
            children,
        })),
        Err(e) => Ok(BackendResultKind::error(format!(
            "Error parsing MachO file: {}",
            e
        ))),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    if std::env::args().len() == 1 {
        let config = config::Config::new()?;
        backend_utils::work_loop!(config.host.as_deref(), config.port, |request| {
            process_request(request, &config)
        })?;
        unreachable!()
    }

    for arg in std::env::args().skip(1) {
        let f = match std::fs::File::open(&arg) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("{}: ERROR: Can't open file: {}", arg, e);
                continue;
            }
        };
        let macho = match MachO::new(f) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{}: ERROR: {}", arg, e);
                continue;
            }
        };

        let mh = &macho.macho_header;
        info!("--- MACH-O HEADER ---");
        info!("CPU type: {}", mh.cputypestr);
        info!("Filetype: {}", mh.filetypestr);
        info!("Number of load cmds: {}", mh.ncmds);
        info!("Size of load cmds: {}", mh.sizeofcmds);
        info!("Flags: {:?}", mh.flagsvec);

        info!("--- LOAD COMMANDS ---");
        for lc in &macho.load_cmds {
            info!("Cmd: {}, Size: {}", lc.cmdstr, lc.cmdsize);
        }

        for (segnum, seg) in macho.segment_cmds.iter().enumerate() {
            info!("--- SEGMENT {} ---", segnum);
            info!("Name: {}", seg.segname);
            info!("Vmaddr: {:#x}", seg.vmaddr);
            info!("Vmsize: {:#x}", seg.vmsize);
            info!("File offset: {}", seg.fileoff);
            info!("File size: {}", seg.filesize);
            info!("Number of sections: {}", seg.nsects);
        }

        for (sectnum, sect) in macho.sections.iter().enumerate() {
            info!("--- SECTION {} ---", sectnum);
            info!("Section name: {}", sect.sectname);
            info!("Segment name: {}", sect.segname);
            info!("Addr: {:#x}", sect.addr);
            info!("Size: {}", sect.size);
            info!("Offset: {}", sect.offset);
        }
    }

    Ok(())
}
