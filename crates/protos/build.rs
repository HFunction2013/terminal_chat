use std::fs;
use std::path::Path;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = tonic_build::configure();

    let protos: Vec<_> = glob::glob("./proto/**/*.proto")?
        .filter_map(|e| e.ok())
        .collect();

    let out_dir = std::env::var("OUT_DIR")?;

    if !protos.is_empty() {
        for proto in &protos {
            println!("cargo:rerun-if-changed={}", proto.display());
        }

        config = config.out_dir(&out_dir);
        config.compile_protos(&protos, &["./proto/"])?;
    }

    let out_path = Path::new(&out_dir);

    // 生成 lib.rs —— 只生成嵌套模块，不生成任何一级扁平模块
    let mut lib_content = String::new();

    if out_path.exists() {
        let mut entries: Vec<_> = fs::read_dir(out_path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("rs")
                    && e.path().file_stem().and_then(|s| s.to_str()) != Some("lib")
            })
            .collect();

        entries.sort_by_key(|e| e.file_name());

        // 收集嵌套关系
        let mut nested_modules: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let mut flat_files: Vec<String> = Vec::new();

        for entry in &entries {
            let file_name = entry.path().file_stem().unwrap().to_string_lossy().to_string();
            
            if file_name.contains('.') {
                let parts: Vec<&str> = file_name.split('.').collect();
                if parts.len() == 2 {
                    let parent = parts[0].to_string();
                    let child = parts[1].to_string();
                    nested_modules.entry(parent)
                        .or_default()
                        .push((child, file_name.clone()));
                }
            } else {
                flat_files.push(file_name);
            }
        }

        // 没有 . 的文件：直接一级 include
        for file_name in &flat_files {
            lib_content.push_str(&format!(
                "pub mod {} {{\n    include!(concat!(env!(\"OUT_DIR\"), \"/{}.rs\"));\n}}\n\n",
                file_name, file_name
            ));
        }

        // 有 . 的文件：直接嵌套 include，不经过一级模块
        for (parent, children) in nested_modules {
            lib_content.push_str(&format!("pub mod {} {{\n", parent));
            for (child, file_name) in children {
                lib_content.push_str(&format!(
                    "    pub mod {} {{\n        include!(concat!(env!(\"OUT_DIR\"), \"/{}.rs\"));\n    }}\n",
                    child, file_name
                ));
            }
            lib_content.push_str("}\n\n");
        }
    }

    let lib_out_path = out_path.join("lib.rs");
    fs::write(&lib_out_path, lib_content)?;

    println!("cargo:rerun-if-changed={}", lib_out_path.display());

    Ok(())
}