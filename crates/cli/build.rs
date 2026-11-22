use built::write_built_file;
use std::fs;
use std::path::PathBuf;

fn main() {
    write_built_file().unwrap();

    // Compile in the CLI bilingual dictionary (en/zh) from the top-level
    // i18n/ directory; the generated module is included by src/main.rs.
    let toml_path = "../../i18n/cli.toml";
    let toml = fs::read_to_string(toml_path).expect("read i18n/cli.toml");
    let code = i18n_build::generate(&toml);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("i18n_cli.rs"), code).expect("write generated i18n module");
    println!("cargo::rerun-if-changed={toml_path}");
}
