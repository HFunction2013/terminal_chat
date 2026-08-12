use proc_macro::TokenStream;
use quote::quote;
use syn::LitStr;
use syn::{
    FnArg, ItemFn, PatType, ReturnType, Type, parse::Parse, parse::ParseStream, parse_macro_input,
};

/// 解析 fallback 参数的辅助结构
struct HookAttr {
    fallback: Option<String>,
}

impl Parse for HookAttr {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(HookAttr { fallback: None });
        }

        // 解析 fallback = "expression"
        let name: syn::Ident = input.parse()?;
        if name != "fallback" {
            return Err(syn::Error::new(name.span(), "expected `fallback`"));
        }

        let _: syn::Token![=] = input.parse()?;
        let value: LitStr = input.parse()?;

        Ok(HookAttr { fallback: Some(value.value()) })
    }
}

#[proc_macro_attribute]
pub fn register_hook(attr: TokenStream, item: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn);

    // 解析属性参数
    let hook_attr = parse_macro_input!(attr as HookAttr);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();

    assert!(
        fn_name_str.ends_with("_impl"),
        "Function with #[register_hook] must end with '_impl', got '{}'",
        fn_name_str
    );

    let wrapper_name_str = fn_name_str.trim_end_matches("_impl");
    let wrapper_name = syn::Ident::new(wrapper_name_str, fn_name.span());
    let fn_name_upper = wrapper_name_str.to_uppercase();

    let inputs = &input_fn.sig.inputs;
    let param_count = inputs.len();

    assert!(param_count <= 1, "Function with #[register_hook] must have at most one parameter");

    let (param_name, param_type, inner_type) = if param_count == 1 {
        match &inputs[0] {
            FnArg::Typed(PatType { pat, ty, .. }) => {
                let name = match pat.as_ref() {
                    syn::Pat::Ident(p) => &p.ident,
                    _ => panic!("Expected simple parameter name"),
                };

                let inner = match ty.as_ref() {
                    Type::Reference(type_ref) => &type_ref.elem,
                    other => other,
                };

                (Some(name.clone()), Some(ty.as_ref()), Some(inner))
            }
            _ => (None, None, None),
        }
    } else {
        (None, None, None)
    };

    let original_output = &input_fn.sig.output;

    generate_hook_system(
        &input_fn,
        fn_name,
        &wrapper_name,
        wrapper_name_str,
        fn_name_upper,
        param_name,
        param_type,
        inner_type,
        original_output,
        param_count > 0,
        hook_attr.fallback.as_deref(), // 传入 fallback
    )
    .into()
}

fn generate_hook_system(
    original_fn: &ItemFn,
    impl_fn_name: &syn::Ident,
    wrapper_fn_name: &syn::Ident,
    wrapper_name_str: &str,
    fn_name_upper: String,
    param_name: Option<syn::Ident>,
    param_type: Option<&Type>,
    inner_type: Option<&Type>,
    original_output: &ReturnType,
    has_param: bool,
    fallback_expr: Option<&str>, // 新增参数
) -> proc_macro2::TokenStream {
    let before_hook_name = syn::Ident::new(
        &format!("Before{}Hook", to_upper_camel_case(wrapper_name_str)),
        wrapper_fn_name.span(),
    );
    let on_hook_name = syn::Ident::new(
        &format!("On{}Hook", to_upper_camel_case(wrapper_name_str)),
        wrapper_fn_name.span(),
    );
    let after_hook_name = syn::Ident::new(
        &format!("After{}Hook", to_upper_camel_case(wrapper_name_str)),
        wrapper_fn_name.span(),
    );

    let before_hooks_static =
        syn::Ident::new(&format!("BEFORE_{}_HOOKS", fn_name_upper), wrapper_fn_name.span());
    let on_hooks_static =
        syn::Ident::new(&format!("ON_{}_HOOKS", fn_name_upper), wrapper_fn_name.span());
    let after_hooks_static =
        syn::Ident::new(&format!("AFTER_{}_HOOKS", fn_name_upper), wrapper_fn_name.span());
    let in_flag_static = syn::Ident::new(&format!("IN_{}", fn_name_upper), wrapper_fn_name.span());

    let register_before =
        syn::Ident::new(&format!("register_before_{}", wrapper_name_str), wrapper_fn_name.span());
    let register_on =
        syn::Ident::new(&format!("register_on_{}", wrapper_name_str), wrapper_fn_name.span());
    let register_after =
        syn::Ident::new(&format!("register_after_{}", wrapper_name_str), wrapper_fn_name.span());
    let clear_hooks =
        syn::Ident::new(&format!("clear_{}_hooks", wrapper_name_str), wrapper_fn_name.span());

    let run_before =
        syn::Ident::new(&format!("run_before_{}_chain", wrapper_name_str), wrapper_fn_name.span());
    let run_on =
        syn::Ident::new(&format!("run_on_{}_chain", wrapper_name_str), wrapper_fn_name.span());
    let run_after =
        syn::Ident::new(&format!("run_after_{}_chain", wrapper_name_str), wrapper_fn_name.span());

    let fn_block = &original_fn.block;
    let fn_vis = &original_fn.vis;
    let fn_attrs = &original_fn.attrs;

    // ==== 最小侵入修改：根据 fallback 参数决定默认值 ====
    let default_result = if let Some(expr) = fallback_expr {
        // 用户指定的 fallback 表达式
        let expr_token: proc_macro2::TokenStream =
            expr.parse().expect("Invalid fallback expression");
        quote! { #expr_token }
    } else {
        // 没指定就用 Default::default()
        quote! { Default::default() }
    };
    // ==== 结束修改 ====

    // 以下代码完全不变...
    if has_param {
        let p_name = param_name.as_ref().unwrap();
        let p_type = param_type.unwrap();
        let p_inner = inner_type.unwrap();

        quote! {
            // ... 全部保持不变，只把 Default::default() 换成 #default_result ...

            #(#fn_attrs)*
            #fn_vis fn #impl_fn_name(#p_name: #p_type) #original_output #fn_block

            type #before_hook_name = unsafe extern "C" fn(*const #p_inner) -> bool;
            type #on_hook_name = unsafe extern "C" fn(*const #p_inner) -> bool;
            type #after_hook_name = unsafe extern "C" fn(*const #p_inner) -> bool;

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

            unsafe fn #run_before(param: *const #p_inner) -> bool {
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

            unsafe fn #run_on(param: *const #p_inner) -> bool {
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

            unsafe fn #run_after(param: *const #p_inner) -> bool {
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

            #[ffi_export]
            pub unsafe fn #wrapper_fn_name(#p_name: #p_type) #original_output {
                let reenter = #in_flag_static.load(Ordering::SeqCst);
                if reenter {
                    return #impl_fn_name(#p_name);
                }
                #in_flag_static.store(true, Ordering::SeqCst);

                let param_ptr: *const #p_inner = #p_name as *const #p_inner;

                let mut interrupted = unsafe { #run_before(param_ptr) };

                if !interrupted {
                    interrupted = unsafe { #run_on(param_ptr) };
                }

                let result = if !interrupted {
                    let res = #impl_fn_name(#p_name);
                    unsafe { #run_after(param_ptr) };
                    res
                } else {
                    #default_result
                };

                #in_flag_static.store(false, Ordering::SeqCst);
                result
            }
        }
    } else {
        quote! {
            #(#fn_attrs)*
            #fn_vis fn #impl_fn_name() #original_output #fn_block

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
            pub unsafe fn #wrapper_fn_name() #original_output {
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
                    #default_result
                };

                #in_flag_static.store(false, Ordering::SeqCst);
                result
            }
        }
    }
}

/// 下划线命名转大驼峰命名
fn to_upper_camel_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}
