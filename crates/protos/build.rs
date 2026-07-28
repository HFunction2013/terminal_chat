use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    let protos: Vec<_> = glob::glob("./proto/**/*.proto")?
        .filter_map(|e| e.ok())
        .collect();

    let out_dir = std::env::var("OUT_DIR")?;

    // 只在有 proto 文件时才编译
    if !protos.is_empty() {
        for proto in &protos {
            println!("cargo:rerun-if-changed={}", proto.display());
        }

        config.out_dir(&out_dir);
        config.compile_protos(&protos, &["./proto/"])?;
    }

    // 生成 OUT_DIR/lib.rs
    let mut lib_content = String::new();

    for proto in &protos {
        let stem = proto.file_stem().unwrap().to_string_lossy();
        lib_content.push_str(&format!(
            "pub mod {0} {{
    include!(concat!(env!(\"OUT_DIR\"), \"/{0}.rs\"));
}}\n\n",
            stem
        ));
    }

    let lib_out_path = Path::new(&out_dir).join("lib.rs");
    fs::write(&lib_out_path, lib_content)?;

    println!("cargo:rerun-if-changed={}", lib_out_path.display());

    Ok(())
}