use ctxole::{
    crypto::OleCrypto,
    oleps::{DocumentSummaryInformation, SummaryInformation},
    Ole, OleEntry,
};
use std::fs::File;
use std::io::{self, BufReader, Write};
use tracing_subscriber::prelude::*;

fn usage(me: &str) -> ! {
    eprintln!("Usage:");
    eprintln!("{} <olefile>", me);
    eprintln!("  Lists all entries in <olefile>");
    eprintln!("{} <olefile> <entry>", me);
    eprintln!("  Prints the details of the Ole <entry> in <olefile>");
    eprintln!("{} <olefile> <stream> <output>", me);
    eprintln!("  Extracts <stream> from <olefile> and writes it to <output>");
    eprintln!("{} <olefile> --test", me);
    eprintln!("  Tests all streams in <olefile>");
    eprintln!("{} <olefile> --summary", me);
    eprintln!("  Prints SummaryInformation and DocumentSummaryInformation from <olefile>");
    eprintln!("{} <olefile> --decrypt", me);
    eprintln!("  Prints encryption information from <olefile>");
    eprintln!("{} <olefile> --decrypt <password> <output>", me);
    eprintln!("  Decrypts encrypted <olefile> with <password> into <output>");
    std::process::exit(1);
}

fn extract_stream(ole: &Ole<BufReader<File>>, entry: &OleEntry, to: &str) -> Result<(), io::Error> {
    let mut reader = ole.get_stream_reader(entry);
    let mut writer: Box<dyn Write> = match to {
        "-" => Box::new(io::stdout()),
        _ => Box::new(File::create(to)?),
    };
    std::io::copy(&mut reader, &mut writer)?;
    Ok(())
}

fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    if !(2..=5).contains(&args.len()) {
        usage(&args[0]);
    }

    let docfname = &args[1];
    let f = File::open(docfname).map_err(|e| {
        eprintln!("Failed to open {}: {}", &docfname, e);
        e
    })?;
    let ole = Ole::new(BufReader::new(f)).map_err(|e| {
        eprintln!("Ole::new failed: {}", e);
        e
    })?;

    if args.len() == 2 {
        for it in ole.ftw() {
            let kind = if it.1.is_storage() {
                "storage"
            } else {
                "stream"
            };
            println!("[{}] {:?}", kind, it.0);
        }
    } else if args.len() == 3 && args[2] == "--test" {
        let anomalies = ole.anomalies();
        if !anomalies.is_empty() {
            eprintln!("Warning: the following Ole defects were encountered");
            for an in anomalies {
                eprintln!("  - {}", an);
            }
        }
        let mut exit_code = 0;
        for (name, entry) in ole.ftw() {
            if !entry.is_storage() {
                let mut reader = ole.get_stream_reader(&entry);
                if let Err(e) = std::io::copy(&mut reader, &mut std::io::sink()) {
                    exit_code = 1;
                    eprintln!("{name}: {e}");
                } else {
                    eprintln!("{name}: OK");
                }
            } else {
                eprintln!("{name}/: OK");
            }
            let anomalies = entry.anomalies.as_slice();
            if !anomalies.is_empty() {
                eprintln!("  Warning: the following Entry defects were encountered");
                for an in anomalies {
                    eprintln!("    - {}", an);
                }
            }
        }
        std::process::exit(exit_code);
    } else if args.len() == 3 && args[2] == "--summary" {
        print_summary_information(&ole).map_err(|e| {
            eprintln!("Summary enumeration failed: {}", e);
            e
        })?;
    } else if [3, 5].contains(&args.len()) && args[2] == "--decrypt" {
        let ole_crypto = OleCrypto::new(&ole).map_err(|e| {
            eprintln!("Encryption not present or not supported: {e}");
            e
        })?;
        if args.len() == 3 {
            eprintln!("{:#?}", ole_crypto);
        } else if let Some(key) = ole_crypto.get_key(&args[3]) {
            eprintln!("Password ok! Key {key:x}");
            let writer: Box<dyn Write> = match args[4].as_str() {
                "-" => Box::new(io::stdout()),
                outf => Box::new(File::create(outf).map_err(|e| {
                    eprintln!("Failed to create output file: {e}");
                    e
                })?),
            };
            ole_crypto.decrypt(&key, &ole, writer).map_err(|e| {
                eprintln!("Failed to write output file: {e}");
                e
            })?;
        } else {
            eprintln!("Wrong password");
        }
    } else {
        let entry = match ole.get_entry_by_name(&args[2]) {
            Ok(v) => v,
            Err(e) => {
                match e.kind() {
                    io::ErrorKind::InvalidData => {
                        eprintln!("An Ole parse problem was encountered: {}", e)
                    }
                    io::ErrorKind::NotFound => {
                        eprintln!("The requested entry could not be found")
                    }
                    _ => {
                        eprintln!("An error occurred: {}", e);
                    }
                }
                return Err(e);
            }
        };
        if args.len() == 3 {
            println!("Details for {}:", entry.name);
            println!("{:#?}", entry);
        } else {
            extract_stream(&ole, &entry, &args[3]).map_err(|e| {
                eprintln!("An error occurred: {}", e);
                e
            })?;
        }
    }
    Ok(())
}

fn print_summary_information<R: io::Read + io::Seek>(ole: &Ole<R>) -> Result<(), io::Error> {
    if let Ok(entry) = ole.get_entry_by_name("\u{5}SummaryInformation") {
        let mut stream = ole.get_stream_reader(&entry);
        match SummaryInformation::new(&mut stream) {
            Ok(summaryinfo) => println!("{:#?}", summaryinfo),
            Err(e) => println!("Failed to parse SummaryInformation: {e}"),
        }
    } else {
        println!("SummaryInformation stream not found");
    }

    if let Ok(entry) = ole.get_entry_by_name("\u{5}DocumentSummaryInformation") {
        let mut stream = ole.get_stream_reader(&entry);
        match DocumentSummaryInformation::new(&mut stream) {
            Ok(dsi) => println!("{:#?}", dsi),
            Err(e) => println!("Failed to parse DocumentSummaryInformation: {e}"),
        }
    } else {
        println!("DocumentSummaryInformation stream not found");
    }
    Ok(())
}
