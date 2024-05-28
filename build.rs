fn main() {
    // Used only until `doc_auto_cfg` is stable, see also: https://stackoverflow.com/a/70914430
    #[rustversion::nightly]
    fn set_nightly_cfg() {
        println!("cargo:rustc-cfg=nightly")
    }
    #[rustversion::not(nightly)]
    fn set_nightly_cfg() {}

    set_nightly_cfg();
    println!("cargo::rustc-check-cfg=cfg(nightly)");
}
