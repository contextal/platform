use std::{env, path::PathBuf};

fn main() {
    let pkg_conf = pkg_config::Config::new()
        .atleast_version("4.0")
        .probe("tesseract")
        .unwrap();

    pkg_conf
        .link_paths
        .iter()
        .for_each(|path| println!("cargo:rustc-link-search={}", path.to_string_lossy()));

    pkg_conf
        .libs
        .iter()
        .for_each(|lib| println!("cargo:rustc-link-lib={lib}"));

    println!("cargo:rerun-if-changed=wrapper.h");

    let bindings = bindgen::Builder::default()
        .default_enum_style(bindgen::EnumVariation::Rust {
            non_exhaustive: true,
        })
        .header("wrapper.h")
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args(
            pkg_conf
                .include_paths
                .iter()
                .map(|path| format!("-I{}", path.to_string_lossy())),
        )
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
