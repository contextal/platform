use ooxml::{
    BinaryWorkbook, Document, Ooxml, OoxmlError, ProcessingSummary, Wordprocessing, Workbook,
};
use std::{
    fs,
    io::{Read, Seek},
};
use tracing_subscriber::prelude::*;

fn main() -> Result<(), OoxmlError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let args: Vec<String> = std::env::args().collect();
    let fname = &args[1];
    let shared_strings_cache_limit = 10_000_000;
    let mut ooxml = match Ooxml::new(fs::File::open(fname)?, shared_strings_cache_limit) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to open {}: {}", fname, e);
            std::process::exit(1);
        }
    };

    match &mut ooxml.document {
        Document::Docx(wordprocessing) => process_wordprocessing(wordprocessing),
        Document::Xlsx(spreadsheet) => process_spreadsheet(spreadsheet),
        Document::Xlsb(xlsb) => process_binary_workbook(xlsb),
    };

    println!("\n\nMETADATA:\n{:#?}", ooxml.properties);
    Ok(())
}

fn process_wordprocessing<R: Read + Seek>(doc: &mut Wordprocessing<R>) {
    let mut output = Vec::<u8>::new();
    let mut result = ProcessingSummary::default();
    doc.process(&mut output, &mut result).unwrap();
    println!("\n\nRELATIONSHIPS:");
    for relationship in doc.relationships() {
        println!(
            "id={} target={:?} type={:?}",
            relationship.id, relationship.target, relationship.rel_type
        );
    }
    println!("\n\nHYPERLINKS:");
    for hyperlink in result.hyperlinks {
        println!("{hyperlink}");
    }
    println!("\n\nFILES TO PROCESS:");
    for file in result.files_to_process {
        println!("{} - {:?}", file.path, file.rel_type);
    }

    println!("\n\nWORD DOCUMENT PROTECTION:");
    for pair in to_sorted_vec(doc.protection()) {
        println!("{}: {}", pair.0, pair.1);
    }

    let str = String::from_utf8(output).unwrap();
    println!("\n\nOUTPUT:\n{str}");
}

fn to_sorted_vec<K: Ord, V>(map: &std::collections::HashMap<K, V>) -> Vec<(&K, &V)> {
    let mut res: Vec<(&K, &V)> = map.iter().collect();
    res.sort_by(|a, b| a.0.cmp(b.0));
    res
}

fn process_spreadsheet<R: Read + Seek>(spreadsheet: &mut Workbook<R>) {
    let mut files_to_process = spreadsheet.files_to_process().to_vec();

    for mut sheet in spreadsheet.iter() {
        println!(
            "Sheet {:?}: {} ({})",
            sheet.info().sheet_type,
            sheet.info().name,
            sheet.info().state
        );

        let mut output = Vec::<u8>::new();
        let mut result = ProcessingSummary::default();
        sheet.process(&mut output, &mut result).unwrap();

        println!("{result:#?}");

        println!("\n\nSHEET PROTECTION:");
        for pair in to_sorted_vec(&result.protection) {
            println!("{}: {}", pair.0, pair.1);
        }

        let str = String::from_utf8(output).unwrap();
        println!("\n\nOUTPUT:\n{str}");

        files_to_process.append(&mut result.files_to_process);
    }

    println!("\n\nWORKBOOK PROTECTION:");
    for pair in to_sorted_vec(spreadsheet.protection()) {
        println!("{}: {}", pair.0, pair.1);
    }

    println!("\n\nFILES TO PROCESS:");
    for file in files_to_process {
        println!("{} - {:?}", file.path, file.rel_type);
    }
}

fn process_binary_workbook<R: Read + Seek>(spreadsheet: &mut BinaryWorkbook<R>) {
    let mut files_to_process = spreadsheet.files_to_process().to_vec();

    for mut sheet in spreadsheet.iter() {
        println!(
            "Sheet {:?}: {} ({})",
            sheet.info().sheet_type,
            sheet.info().name,
            sheet.info().state
        );

        let mut output = Vec::<u8>::new();
        let mut result = ProcessingSummary::default();
        sheet.process(&mut output, &mut result).unwrap();

        println!("{result:#?}");

        println!("\n\nSHEET PROTECTION:");
        for pair in to_sorted_vec(&result.protection) {
            println!("{}: {}", pair.0, pair.1);
        }

        let str = String::from_utf8(output).unwrap();
        println!("\n\nOUTPUT:\n{str}");

        files_to_process.append(&mut result.files_to_process);
    }

    println!("\n\nWORKBOOK PROTECTION:");
    for pair in to_sorted_vec(spreadsheet.protection()) {
        println!("{}: {}", pair.0, pair.1);
    }

    println!("\n\nFILES TO PROCESS:");
    for file in files_to_process {
        println!("{} - {:?}", file.path, file.rel_type);
    }
}
