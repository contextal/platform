use std::{env, path::PathBuf};

fn main() {
    // `vendor/unrar` directory contains unmodified UnRAR source code.
    // The code may not be used to develop a RAR (WinRAR) compatible archiver.
    // See `vendor/unrar/readme.txt` and `vendor/unrar/license.txt` for more details.
    let unrar_files: Vec<_> = vec![
        "archive.cpp",
        "arcread.cpp",
        "blake2s.cpp",
        "cmddata.cpp",
        "consio.cpp",
        "crc.cpp",
        "crypt.cpp",
        "dll.cpp",
        "encname.cpp",
        "errhnd.cpp",
        "extinfo.cpp",
        "extract.cpp",
        "filcreat.cpp",
        "file.cpp",
        "filefn.cpp",
        "filestr.cpp",
        "find.cpp",
        "getbits.cpp",
        "global.cpp",
        "hash.cpp",
        "headers.cpp",
        "list.cpp",
        "match.cpp",
        "options.cpp",
        "pathfn.cpp",
        "qopen.cpp",
        "rar.cpp",
        "rarvm.cpp",
        "rawread.cpp",
        "rdwrfn.cpp",
        "resource.cpp",
        "rijndael.cpp",
        "rs16.cpp",
        "scantree.cpp",
        "secpassword.cpp",
        "sha1.cpp",
        "sha256.cpp",
        "smallfn.cpp",
        "strfn.cpp",
        "strlist.cpp",
        "system.cpp",
        "threadpool.cpp",
        "timefn.cpp",
        "ui.cpp",
        "unicode.cpp",
        "unpack.cpp",
        "volume.cpp",
    ]
    .into_iter()
    .map(|file| format!("vendor/unrar/{file}"))
    .inspect(|file| println!("cargo:rerun-if-changed={file}"))
    .collect();

    cc::Build::new()
        .cpp(true)
        .pic(true)
        .std("c++11")
        .opt_level(2)
        .flag("-Wno-dangling-else")
        .flag_if_supported("-Wno-logical-op-parentheses")
        .flag_if_supported("-Wno-parentheses")
        .flag_if_supported("-Wno-class-memaccess")
        .flag_if_supported("-Wno-misleading-indentation")
        .flag_if_supported("-Wno-comment")
        .flag_if_supported("-Wno-extra")
        .flag("-Wno-missing-braces")
        .flag("-Wno-missing-field-initializers")
        .flag("-Wno-sign-compare")
        .flag("-Wno-switch")
        .flag("-Wno-unused-but-set-variable")
        .flag("-Wno-unused-function")
        .flag("-Wno-unused-parameter")
        .flag("-Wno-unused-variable")
        .define("_FILE_OFFSET_BITS", Some("64"))
        .define("_LARGEFILE_SOURCE", None)
        .define("RAR_SMP", None)
        .define("RARDLL", None)
        .files(unrar_files)
        .compile("libunrar.a");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    let bindings = bindgen::Builder::default()
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: true,
        })
        .header("wrapper.hpp")
        .derive_default(true)
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .blocklist_item("RAROpenArchive")
        .blocklist_item("RAROpenArchiveData")
        .blocklist_item("RARReadHeader")
        .blocklist_item("RARHeaderData")
        .blocklist_item("RARProcessFile")
        .blocklist_item("RARSetProcessDataProc")
        .blocklist_item("RARSetChangeVolProc")
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
