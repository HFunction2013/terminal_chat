use std::fs;
use std::path::PathBuf;

fn main() {
    let yaml_path = PathBuf::from("./commands.yaml");
    let yaml = fs::read_to_string(&yaml_path).expect("Cannot read ./commands.yaml, file missing");
    yaml2cmd_build::build_with_yaml(&yaml);
    println!("cargo:rerun-if-changed={}", yaml_path.display());

    // Compile in the plugin bilingual dictionary (en/zh) from the top-level
    // i18n/ directory; the generated module is included by src/lib.rs.
    let toml_path = "../../../i18n/cli-standard.toml";
    let toml = fs::read_to_string(toml_path).expect("read i18n/cli-standard.toml");
    let code = i18n_build::generate(&toml);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("i18n_std.rs"), code).expect("write generated i18n module");
    println!("cargo:rerun-if-changed={toml_path}");
}
