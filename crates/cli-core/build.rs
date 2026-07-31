fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-install_name,@rpath/libcli_core.dylib");
    }
}