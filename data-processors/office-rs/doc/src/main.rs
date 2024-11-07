use ctxole::Ole;
use doc::{Doc, DocPart, WordChar};
use std::fs::File;
use std::io::BufReader;
use tracing_subscriber::prelude::*;

fn usage(me: &str) -> ! {
    eprintln!("Usage:");
    eprintln!("{} <docfile> --text", me);
    eprintln!("{} <docfile> --properties", me);
    std::process::exit(0);
}

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        usage(&args[0]);
    }

    let docf = match File::open(&args[1]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to open {}: {}", args[1], e);
            std::process::exit(1);
        }
    };

    let ole = Ole::new(BufReader::new(docf)).unwrap_or_else(|e| {
        eprintln!("Failed to parse {}: {}", args[1], e);
        std::process::exit(1);
    });
    let mut doc = Doc::new(
        ole,
        &[
            "contextal",
            "Password1234_",
            "openwall",
            "hashcat",
            "1234567890123456",
            "123456789012345",
            "myhovercraftisfullofeels",
            "myhovercraftisf",
        ],
    )
    .unwrap();
    match args[2].as_str() {
        "--text" => {
            let it = doc.char_iter(DocPart::MainDocument).unwrap_or_else(|e| {
                eprintln!("Failed to extract MainDocument chars: {}", e);
                std::process::exit(1);
            });
            for c in it {
                match c {
                    WordChar::Char(c) => {
                        print!("{c}");
                    }
                    WordChar::ComplexField { value, .. } => print!("{value}"),
                    WordChar::Hyperlink { text, .. } => print!("{text}"),
                    other => print!(" {other:?} "),
                }
            }
        }
        "--debug" => {
            let it = doc.char_iter(DocPart::MainDocument).unwrap_or_else(|e| {
                eprintln!("Failed to extract MainDocument chars: {}", e);
                std::process::exit(1);
            });
            for c in it {
                match c {
                    WordChar::Char(c) => {
                        println!("{:04x} {c}", c as u16);
                    }
                    other => println!("{other:?}"),
                }
            }
        }
        "--properties" => {
            if let Some(dop) = doc.get_dop() {
                println!("{:#?}", dop)
            }
            if let Some(assocs) = doc.get_associations() {
                println!("{:#?}", assocs)
            }
        }
        _ => usage(&args[0]),
    }
}
