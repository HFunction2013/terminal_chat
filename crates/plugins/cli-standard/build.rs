use std::path::Path;
use std::fs;
fn main() {
    let yaml_path = Path::new("./commands.yaml");
    let yaml = fs::read_to_string(yaml_path).expect("Cannot read ./commands.yaml, file missing");
    yaml2cmd_build::build_with_yaml(yaml);
    println!("cargo:rerun-if-changed={}", yaml_path.display());
}
