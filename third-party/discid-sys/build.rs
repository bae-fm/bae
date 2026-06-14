extern crate bindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=discid");

    let bindings = bindgen::Builder::default()
        .header("discid.h")
        .bitfield_enum("discid_feature")
        .blocklist_item("DISCID_VERSION_MAJOR")
        .blocklist_item("DISCID_VERSION_MINOR")
        .blocklist_item("DISCID_VERSION_PATCH")
        .blocklist_item("DISCID_VERSION_NUM")
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
