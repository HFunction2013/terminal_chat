use std::env;
use std::fs;
use std::path::Path;
use syn::{FnArg, GenericArgument, Item, PathArguments, ReturnType, Type, parse_file};

/// 一个被导出函数的参数：类型串 + 是否为 `Out<'_, T>` 出参。
struct Param {
    ty: String,
    is_out: bool,
}

/// 判断类型是否为 `Out<'_, T>` 出参（末段标识为 `Out`，兼容全路径）。
fn is_out_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        type_path.path.segments.last().map(|seg| seg.ident == "Out").unwrap_or(false)
    } else {
        false
    }
}

/// 剥掉引用外层（`&` / `mut` / 生命周期），取业务参数的内部类型，供 Hook 指针使用。
fn inner_of(ty: &str) -> String {
    let t = ty.trim_start();
    let t = t.strip_prefix("&").unwrap_or(t);
    let t = t.trim_start();
    let t = t.strip_prefix("mut ").unwrap_or(t);
    let t = t.trim_start();
    // 去掉前导生命周期 `'_ ` / `'a `。
    let t = t
        .strip_prefix('\'')
        .map(|rest| rest.split_once(' ').map(|(_, r)| r).unwrap_or(rest))
        .unwrap_or(t);
    t.trim().to_string()
}

#[allow(clippy::type_complexity)]
fn find_register_hook_functions<P: AsRef<Path>>(
    path: P,
) -> Result<Vec<(String, Vec<Param>, String)>, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(&path)?;
    let syntax_tree = parse_file(&content)?;

    let mut result = Vec::new();

    for item in &syntax_tree.items {
        if let Item::Fn(func) = item {
            let has_hook = func.attrs.iter().any(|attr| attr.path().is_ident("register_hook"));

            if !has_hook {
                continue;
            }

            let params: Vec<Param> = func
                .sig
                .inputs
                .iter()
                .filter_map(|arg| match arg {
                    FnArg::Typed(pat_type) => Some(Param {
                        ty: type_to_string(&pat_type.ty),
                        is_out: is_out_type(&pat_type.ty),
                    }),
                    FnArg::Receiver(_) => None,
                })
                .collect();

            let return_type = match &func.sig.output {
                ReturnType::Default => "()".to_string(),
                ReturnType::Type(_, ty) => type_to_string(ty),
            };

            result.push((func.sig.ident.to_string(), params, return_type));
        }
    }

    Ok(result)
}

fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|seg| {
                    let ident = seg.ident.to_string();
                    let args = path_args_to_string(&seg.arguments);
                    format!("{}{}", ident, args)
                })
                .collect();
            segments.join("::")
        }
        Type::Tuple(tuple) => {
            if tuple.elems.is_empty() {
                "()".to_string()
            } else {
                let elems: Vec<String> = tuple.elems.iter().map(type_to_string).collect();
                format!("({})", elems.join(", "))
            }
        }
        Type::Reference(ref_type) => {
            let lifetime =
                ref_type.lifetime.as_ref().map(|lt| lt.ident.to_string() + " ").unwrap_or_default();
            let mutability = if ref_type.mutability.is_some() { "mut " } else { "" };
            format!("&{}{}{}", lifetime, mutability, type_to_string(&ref_type.elem))
        }
        _ => format!("{:?}", ty),
    }
}
fn path_args_to_string(args: &PathArguments) -> String {
    match args {
        PathArguments::None => String::new(),
        PathArguments::AngleBracketed(angle) => {
            let generics: Vec<String> = angle
                .args
                .iter()
                .map(|arg| match arg {
                    GenericArgument::Type(ty) => type_to_string(&ty.clone()),
                    GenericArgument::Lifetime(lt) => format!("'{}", lt.ident.to_string()),
                    GenericArgument::Const(expr) => {
                        format!("{}", quote::ToTokens::to_token_stream(expr))
                    }
                    other => format!("{:?}", other),
                })
                .collect();
            format!("<{}>", generics.join(", "))
        }
        PathArguments::Parenthesized(parent) => {
            let inputs: Vec<String> =
                parent.inputs.iter().map(|ty| type_to_string(&ty.clone())).collect();
            let output = match &parent.output {
                ReturnType::Default => String::new(),
                ReturnType::Type(_, ty) => format!(" -> {}", type_to_string(ty)),
            };
            format!("({}){}", inputs.join(", "), output)
        }
    }
}

/// 将 snake_case 转换为 CamelCase
fn to_camel_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// 收集所有 .rs 文件路径
fn collect_rs_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();

    if dir.is_dir()
        && let Ok(entries) = fs::read_dir(dir)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name != "target" && !name.starts_with('.') {
                    files.extend(collect_rs_files(&path));
                }
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }

    files
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");
    let output_path = Path::new(&out_dir).join("fn_type.rs");

    let root_dir = "../cli-core";

    // 告诉 cargo 监控整个 cli-core 目录的变化
    println!("cargo::rerun-if-changed={}", root_dir);

    let rs_files = collect_rs_files(Path::new(root_dir));

    let mut output = String::new();

    // 添加防止重复引入的 guard
    output.push_str("#[allow(non_camel_case_types, dead_code, non_snake_case)]\n");
    output.push_str("// Auto-generated by build.rs - DO NOT EDIT\n");

    // 按文件分组输出注释（机器不读但方便 debug）
    for file_path in &rs_files {
        match find_register_hook_functions(file_path) {
            Ok(functions) => {
                if !functions.is_empty() {
                    output.push_str(&format!("// File: {}\n", file_path.display()));

                    for (name, params, return_type) in &functions {
                        let camel_name = to_camel_case(name);

                        let all_tys: Vec<String> = params.iter().map(|p| p.ty.clone()).collect();
                        let biz: Vec<String> =
                            params.iter().filter(|p| !p.is_out).map(|p| inner_of(&p.ty)).collect();

                        // 1. 函数指针类型：保留全部参数（含 Out 出参）与返回类型。
                        let fn_ptr_type_name = format!("{}Fn", camel_name);
                        let fn_ptr = if return_type == "()" {
                            format!("extern \"C\" fn({})", all_tys.join(", "))
                        } else {
                            format!("extern \"C\" fn({}) -> {}", all_tys.join(", "), return_type)
                        };
                        output.push_str(&format!("pub type {} = {};\n", fn_ptr_type_name, fn_ptr));

                        // 2. Hook 类型：只挂业务参数，Out 出参不进 hook 链。
                        if biz.is_empty() {
                            output.push_str(&format!(
                                "pub type Before{}FnHook = extern \"C\" fn() -> bool;\n",
                                camel_name
                            ));
                            output.push_str(&format!(
                                "pub type On{}FnHook = extern \"C\" fn() -> bool;\n",
                                camel_name
                            ));
                            output.push_str(&format!(
                                "pub type After{}FnHook = extern \"C\" fn() -> bool;\n",
                                camel_name
                            ));
                        } else {
                            let before_args = biz
                                .iter()
                                .map(|b| format!("*mut {}", b))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let const_args = biz
                                .iter()
                                .map(|b| format!("*const {}", b))
                                .collect::<Vec<_>>()
                                .join(", ");
                            output.push_str(&format!(
                                "pub type Before{}FnHook = extern \"C\" fn({}) -> bool;\n",
                                camel_name, before_args
                            ));
                            output.push_str(&format!(
                                "pub type On{}FnHook = extern \"C\" fn({}) -> bool;\n",
                                camel_name, const_args
                            ));
                            output.push_str(&format!(
                                "pub type After{}FnHook = extern \"C\" fn({}) -> bool;\n",
                                camel_name, const_args
                            ));
                        }

                        // 3. 注册函数类型
                        let register_before_type = format!("RegisterBefore{}Fn", camel_name);
                        let register_on_type = format!("RegisterOn{}Fn", camel_name);
                        let register_after_type = format!("RegisterAfter{}Fn", camel_name);

                        let before_hook_type = format!("Before{}FnHook", camel_name);
                        let on_hook_type = format!("On{}FnHook", camel_name);
                        let after_hook_type = format!("After{}FnHook", camel_name);

                        output.push_str(&format!(
                            "pub type {} = extern \"C\" fn({});\n",
                            register_before_type, before_hook_type
                        ));
                        output.push_str(&format!(
                            "pub type {} = extern \"C\" fn({});\n",
                            register_on_type, on_hook_type
                        ));
                        output.push_str(&format!(
                            "pub type {} = extern \"C\" fn({});\n",
                            register_after_type, after_hook_type
                        ));

                        // 4. clear_hooks 函数类型
                        let clear_hooks_type = format!("Clear{}HooksFn", camel_name);
                        output.push_str(&format!(
                            "pub type {} = extern \"C\" fn();\n",
                            clear_hooks_type
                        ));

                        output.push('\n');
                    }
                }
            }
            Err(e) => {
                output.push_str(&format!("// Error parsing {}: {}\n", file_path.display(), e));
            }
        }
    }

    fs::write(&output_path, output)?;

    Ok(())
}
