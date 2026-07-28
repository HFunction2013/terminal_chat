use std::fs;
use std::path::Path;
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = prost_build::Config::new();

    let protos: Vec<_> = glob::glob("./proto/**/*.proto")?
        .filter_map(|e| e.ok())
        .collect();

    let out_dir = std::env::var("OUT_DIR")?;

    if !protos.is_empty() {
        for proto in &protos {
            println!("cargo:rerun-if-changed={}", proto.display());
        }

        config.out_dir(&out_dir);
        config.compile_protos(&protos, &["./proto/"])?;
    }

    // 生成 OUT_DIR/lib.rs
    let mut lib_content = String::new();
    let out_path = Path::new(&out_dir);

    if out_path.exists() {
        let mut entries: Vec<_> = fs::read_dir(out_path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().extension().and_then(|s| s.to_str()) == Some("rs")
                    && e.path().file_stem().and_then(|s| s.to_str()) != Some("lib")
            })
            .collect();
        
        entries.sort_by_key(|e| e.file_name());

        // 生成模块声明
        for entry in &entries {
            let path = entry.path();
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            let module_name = file_name.replace('.', "_");
            
            lib_content.push_str(&format!(
                "pub mod {0} {{
    include!(concat!(env!(\"OUT_DIR\"), \"/{1}.rs\"));
}}\n\n",
                module_name, file_name
            ));
        }

        // 收集所有需要生成嵌套模块的信息
        // 使用 HashMap 按父模块名聚合子模块
        let mut nested_modules: HashMap<String, Vec<(String, String)>> = HashMap::new();
        
        for entry in &entries {
            let path = entry.path();
            let file_name = path.file_stem().unwrap().to_string_lossy().to_string();
            
            if file_name.contains('.') {
                let parts: Vec<&str> = file_name.split('.').collect();
                if parts.len() == 2 {
                    let parent = parts[0].to_string();
                    let child = parts[1].to_string();
                    let flat_module = file_name.replace('.', "_");
                    
                    // 检查父模块是否已经被声明（作为扁平模块）
                    let parent_as_flat = entries.iter().any(|e| {
                        e.path().file_stem().and_then(|s| s.to_str()) == Some(&parent)
                    });
                    
                    if !parent_as_flat {
                        nested_modules.entry(parent)
                            .or_default()
                            .push((child, flat_module));
                    }
                }
            }
        }

        // 生成嵌套模块（已按父模块名合并）
        for (parent, children) in nested_modules {
            lib_content.push_str(&format!("pub mod {} {{\n", parent));
            for (child, flat_module) in children {
                lib_content.push_str(&format!(
                    "    pub mod {} {{\n        pub use super::super::{}::*;\n    }}\n",
                    child, flat_module
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