use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, FnArg, PatType, Type, ReturnType};

/// 属性宏：自动为带 _impl 后缀的函数生成钩子系统
/// 
/// 用法：
/// #[register_hook]
/// fn run_command_impl(args: &repr_c::Vec<repr_c::String>) -> Result {
///     // 实际逻辑
/// }
/// 
/// 会生成：
/// - run_command_impl 保持不变（实际逻辑）
/// - run_command 作为包装器（带钩子系统）
#[proc_macro_attribute]
pub fn register_hook(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();
    
    // 验证函数名以 _impl 结尾
    assert!(
        fn_name_str.ends_with("_impl"),
        "Function with #[register_hook] must end with '_impl', got '{}'",
        fn_name_str
    );
    
    // 去掉 _impl 后缀得到包装函数名
    let wrapper_name_str = fn_name_str.trim_end_matches("_impl");
    let wrapper_name = syn::Ident::new(wrapper_name_str, fn_name.span());
    let fn_name_upper = wrapper_name_str.to_uppercase();
    
    // 获取函数参数（最多一个）
    let inputs = &input_fn.sig.inputs;
    let param_count = inputs.len();
    
    assert!(
        param_count <= 1,
        "Function with #[register_hook] must have at most one parameter"
    );
    
    // 提取参数名和类型
    let (param_name, param_type) = if param_count == 1 {
        match &inputs[0] {
            FnArg::Typed(PatType { pat, ty, .. }) => {
                let name = match pat.as_ref() {
                    syn::Pat::Ident(p) => &p.ident,
                    _ => panic!("Expected simple parameter name"),
                };
                (Some(name.clone()), Some(ty.as_ref()))
            },
            _ => (None, None),
        }
    } else {
        (None, None)
    };
    
    let return_type = match &input_fn.sig.output {
        ReturnType::Type(_, ty) => Some(ty.as_ref()),
        ReturnType::Default => None,
    };
    
    generate_hook_system(
        &input_fn,
        fn_name,
        &wrapper_name,
        wrapper_name_str,
        fn_name_upper,
        param_name,
        param_type,
        return_type,
        param_count > 0,
    ).into()
}

fn generate_hook_system(
    original_fn: &ItemFn,
    impl_fn_name: &syn::Ident,
    wrapper_fn_name: &syn::Ident,
    wrapper_name_str: &str,
    fn_name_upper: String,
    param_name: Option<syn::Ident>,
    param_type: Option<&Type>,
    return_type: Option<&Type>,
    has_param: bool,
) -> proc_macro2::TokenStream {
    // 钩子类型名称
    let before_hook_name = syn::Ident::new(
        &format!("Before{}Hook", capitalize_first(wrapper_name_str)),
        wrapper_fn_name.span(),
    );
    let on_hook_name = syn::Ident::new(
        &format!("On{}Hook", capitalize_first(wrapper_name_str)),
        wrapper_fn_name.span(),
    );
    let after_hook_name = syn::Ident::new(
        &format!("After{}Hook", capitalize_first(wrapper_name_str)),
        wrapper_fn_name.span(),
    );
    
    // 静态变量名称
    let before_hooks_static = syn::Ident::new(
        &format!("BEFORE_{}_HOOKS", fn_name_upper),
        wrapper_fn_name.span(),
    );
    let on_hooks_static = syn::Ident::new(
        &format!("ON_{}_HOOKS", fn_name_upper),
        wrapper_fn_name.span(),
    );
    let after_hooks_static = syn::Ident::new(
        &format!("AFTER_{}_HOOKS", fn_name_upper),
        wrapper_fn_name.span(),
    );
    let in_flag_static = syn::Ident::new(
        &format!("IN_{}", fn_name_upper),
        wrapper_fn_name.span(),
    );
    
    // 注册函数名
    let register_before = syn::Ident::new(
        &format!("register_before_{}", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    let register_on = syn::Ident::new(
        &format!("register_on_{}", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    let register_after = syn::Ident::new(
        &format!("register_after_{}", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    let clear_hooks = syn::Ident::new(
        &format!("clear_{}_hooks", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    
    // 运行链函数名
    let run_before = syn::Ident::new(
        &format!("run_before_{}_chain", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    let run_on = syn::Ident::new(
        &format!("run_on_{}_chain", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    let run_after = syn::Ident::new(
        &format!("run_after_{}_chain", wrapper_name_str),
        wrapper_fn_name.span(),
    );
    
    // 保留原函数
    let fn_block = &original_fn.block;
    let fn_vis = &original_fn.vis;
    let fn_attrs = &original_fn.attrs;
    
    // 根据是否有参数生成不同代码
    if has_param {
        let p_name = param_name.as_ref().unwrap();
        let p_type = param_type.unwrap();
        
        quote! {
            // 保持原函数不变（带 _impl 后缀）
            #(#fn_attrs)*
            #fn_vis fn #impl_fn_name(#p_name: #p_type) -> #return_type #fn_block

            // 钩子类型定义
            type #before_hook_name = unsafe extern "C" fn(#p_type) -> bool;
            type #on_hook_name = unsafe extern "C" fn(#p_type) -> bool;
            type #after_hook_name = unsafe extern "C" fn(#p_type) -> bool;

            static #before_hooks_static: LazyLock<Mutex<Vec<#before_hook_name>>> =
                LazyLock::new(|| Mutex::new(Vec::new()));
            static #on_hooks_static: LazyLock<Mutex<Vec<#on_hook_name>>> =
                LazyLock::new(|| Mutex::new(Vec::new()));
            static #after_hooks_static: LazyLock<Mutex<Vec<#after_hook_name>>> =
                LazyLock::new(|| Mutex::new(Vec::new()));

            static #in_flag_static: AtomicBool = AtomicBool::new(false);

            #[ffi_export]
            pub fn #register_before(hook: #before_hook_name) {
                if let Ok(mut hooks) = #before_hooks_static.lock() {
                    hooks.push(hook);
                }
            }

            #[ffi_export]
            pub fn #register_on(hook: #on_hook_name) {
                if let Ok(mut hooks) = #on_hooks_static.lock() {
                    hooks.push(hook);
                }
            }

            #[ffi_export]
            pub fn #register_after(hook: #after_hook_name) {
                if let Ok(mut hooks) = #after_hooks_static.lock() {
                    hooks.push(hook);
                }
            }

            #[ffi_export]
            pub fn #clear_hooks() {
                let _ = #before_hooks_static.lock().map(|mut h| h.clear());
                let _ = #on_hooks_static.lock().map(|mut h| h.clear());
                let _ = #after_hooks_static.lock().map(|mut h| h.clear());
            }

            unsafe fn #run_before(param: #p_type) -> bool {
                if let Ok(list) = #before_hooks_static.lock() {
                    for h in list.iter() {
                        unsafe {
                            if h(param) {
                                return true;
                            }
                        }
                    }
                }
                false
            }

            unsafe fn #run_on(param: #p_type) -> bool {
                if let Ok(list) = #on_hooks_static.lock() {
                    for h in list.iter() {
                        unsafe {
                            if h(param) {
                                return true;
                            }
                        }
                    }
                }
                false
            }

            unsafe fn #run_after(param: #p_type) -> bool {
                if let Ok(list) = #after_hooks_static.lock() {
                    for h in list.iter() {
                        unsafe {
                            if h(param) {
                                return true;
                            }
                        }
                    }
                }
                false
            }

            // 生成不带 _impl 的包装函数
            #[ffi_export]
            pub unsafe fn #wrapper_fn_name(#p_name: #p_type) -> #return_type {
                let reenter = #in_flag_static.load(Ordering::SeqCst);
                if reenter {
                    return #impl_fn_name(#p_name);
                }
                #in_flag_static.store(true, Ordering::SeqCst);

                let mut interrupted = unsafe { #run_before(#p_name) };

                if !interrupted {
                    interrupted = unsafe { #run_on(#p_name) };
                }

                let result = if !interrupted {
                    let res = #impl_fn_name(#p_name);
                    unsafe { #run_after(#p_name) };
                    res
                } else {
                    Default::default()
                };

                #in_flag_static.store(false, Ordering::SeqCst);
                result
            }
        }
    } else {
        quote! {
            // 保持原函数不变
            #(#fn_attrs)*
            #fn_vis fn #impl_fn_name() -> #return_type #fn_block

            type #before_hook_name = unsafe extern "C" fn() -> bool;
            type #on_hook_name = unsafe extern "C" fn() -> bool;
            type #after_hook_name = unsafe extern "C" fn() -> bool;

            static #before_hooks_static: LazyLock<Mutex<Vec<#before_hook_name>>> =
                LazyLock::new(|| Mutex::new(Vec::new()));
            static #on_hooks_static: LazyLock<Mutex<Vec<#on_hook_name>>> =
                LazyLock::new(|| Mutex::new(Vec::new()));
            static #after_hooks_static: LazyLock<Mutex<Vec<#after_hook_name>>> =
                LazyLock::new(|| Mutex::new(Vec::new()));

            static #in_flag_static: AtomicBool = AtomicBool::new(false);

            #[ffi_export]
            pub fn #register_before(hook: #before_hook_name) {
                if let Ok(mut hooks) = #before_hooks_static.lock() {
                    hooks.push(hook);
                }
            }

            #[ffi_export]
            pub fn #register_on(hook: #on_hook_name) {
                if let Ok(mut hooks) = #on_hooks_static.lock() {
                    hooks.push(hook);
                }
            }

            #[ffi_export]
            pub fn #register_after(hook: #after_hook_name) {
                if let Ok(mut hooks) = #after_hooks_static.lock() {
                    hooks.push(hook);
                }
            }

            #[ffi_export]
            pub fn #clear_hooks() {
                let _ = #before_hooks_static.lock().map(|mut h| h.clear());
                let _ = #on_hooks_static.lock().map(|mut h| h.clear());
                let _ = #after_hooks_static.lock().map(|mut h| h.clear());
            }

            unsafe fn #run_before() -> bool {
                if let Ok(list) = #before_hooks_static.lock() {
                    for h in list.iter() {
                        unsafe {
                            if h() {
                                return true;
                            }
                        }
                    }
                }
                false
            }

            unsafe fn #run_on() -> bool {
                if let Ok(list) = #on_hooks_static.lock() {
                    for h in list.iter() {
                        unsafe {
                            if h() {
                                return true;
                            }
                        }
                    }
                }
                false
            }

            unsafe fn #run_after() -> bool {
                if let Ok(list) = #after_hooks_static.lock() {
                    for h in list.iter() {
                        unsafe {
                            if h() {
                                return true;
                            }
                        }
                    }
                }
                false
            }

            #[ffi_export]
            pub unsafe fn #wrapper_fn_name() -> #return_type {
                let reenter = #in_flag_static.load(Ordering::SeqCst);
                if reenter {
                    return #impl_fn_name();
                }
                #in_flag_static.store(true, Ordering::SeqCst);

                let mut interrupted = unsafe { #run_before() };

                if !interrupted {
                    interrupted = unsafe { #run_on() };
                }

                let result = if !interrupted {
                    let res = #impl_fn_name();
                    unsafe { #run_after() };
                    res
                } else {
                    Default::default()
                };

                #in_flag_static.store(false, Ordering::SeqCst);
                result
            }
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}