fn main() {
    let rime = pkg_config::Config::new()
        .atleast_version("1.13")
        .probe("rime")
        .expect("Install librime development headers and Luna Pinyin dictionaries");
    pkg_config::probe_library("libhangul").expect("Install libhangul development headers");
    pkg_config::probe_library("anthy-unicode")
        .expect("Install Anthy Unicode development headers including libanthyinput-unicode");
    println!("cargo:rustc-link-lib=anthyinput-unicode");
    let mut build = cc::Build::new();
    for path in rime.include_paths {
        build.include(path);
    }
    build
        .file("src/ime/rime_shim.c")
        .warnings(true)
        .compile("novakeys_rime");
    println!("cargo:rerun-if-changed=src/ime/rime_shim.c");
}
