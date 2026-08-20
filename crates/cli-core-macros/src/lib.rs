use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Ident, Token, punctuated::Punctuated};

struct LoadSymbolsInput {
    lib_expr: syn::Expr,
    symbols: Punctuated<Ident, Token![,]>,
}

impl syn::parse::Parse for LoadSymbolsInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let lib_expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let symbols = Punctuated::parse_separated_nonempty(input)?;
        Ok(LoadSymbolsInput { lib_expr, symbols })
    }
}

#[proc_macro]
pub fn load_core_symbols(input: TokenStream) -> TokenStream {
    let LoadSymbolsInput { lib_expr, symbols } = parse_macro_input!(input as LoadSymbolsInput);
    
    let expanded = symbols.iter().map(|sym| {
        // 将 snake_case 转换为 PascalCase
        let name_str = sym.to_string();
        let pascal_name = snake_to_pascal(&name_str);
        let type_name = Ident::new(&format!("{}Fn", pascal_name), sym.span());
        let func_name = Ident::new(&name_str, sym.span());
        
        quote! {
            let #func_name: libloading::Symbol<cli_core_types::#type_name> = unsafe {
                #lib_expr
                    .get::<cli_core_types::#type_name>(concat!(#name_str, "\0").as_bytes())
                    .expect(concat!("Failed to get `", #name_str, "`"))
            };
        }
    });

    quote! {
        #(#expanded)*
    }.into()
}

fn snake_to_pascal(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}