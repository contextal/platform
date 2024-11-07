use ctxole::Ole;
use std::fs::File;
use std::io::BufReader;
use tracing_subscriber::prelude::*;
use xls::*;

fn main() -> Result<(), ExcelError> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    let fname = &args[1];
    let docf = match File::open(fname) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to open {}: {}", fname, e);
            std::process::exit(1);
        }
    };
    let ole = Ole::new(BufReader::new(docf)).unwrap_or_else(|e| {
        eprintln!("Failed to parse {}: {}", fname, e);
        std::process::exit(1);
    });

    const PASSWORDS: &[&str] = &[
        "contextal",
        "123456789012345",
        "myhovercraftisfullofeels",
        "myhovercraftisf",
        XLS_DEFAULT_PASSWORD,
    ];

    let mut xls = Xls::new(&ole, PASSWORDS)?;
    println!("{:#?}", xls);

    let workbook = &xls.workbook;

    println!("===WORKSHEETS===");
    for worksheet in workbook.worksheets() {
        let mut output = Vec::<u8>::new();
        let mut processing_result = worksheet::ProcessingResult::default();
        worksheet.process(&mut output, &mut processing_result)?;
        println!("{:#?}", processing_result);
        let output = String::from_utf8(output).unwrap();
        println!("OUTPUT\n{output}");
    }

    println!("===MACROSHEETS===");
    for macrosheet in workbook.macrosheets() {
        let mut output = Vec::<u8>::new();
        let mut processing_result = worksheet::ProcessingResult::default();
        macrosheet.process(&mut output, &mut processing_result)?;
        println!("{:#?}", processing_result);
        let output = String::from_utf8(output).unwrap();
        println!("OUTPUT\n{output}");
    }

    println!("===SUBSTREAMS===");
    for substream in xls.substreams() {
        println!("{substream:?}");
        for record in substream.get_iterator()? {
            println!("{record:?}")
        }
    }

    Ok(())
}
