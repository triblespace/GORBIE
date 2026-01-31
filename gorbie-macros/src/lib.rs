use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Ident, ItemFn, LitStr, Result, Token};

struct NotebookAttr {
    name: Option<LitStr>,
}

impl Parse for NotebookAttr {
    fn parse(input: ParseStream) -> Result<Self> {
        if input.is_empty() {
            return Ok(Self { name: None });
        }

        let name_key: Ident = input.parse()?;
        if name_key != "name" {
            return Err(input.error("expected `name = \"...\"`"));
        }
        input.parse::<Token![=]>()?;
        let name: LitStr = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens"));
        }

        Ok(Self { name: Some(name) })
    }
}

#[proc_macro_attribute]
pub fn notebook(attr: TokenStream, item: TokenStream) -> TokenStream {
    let NotebookAttr { name } = parse_macro_input!(attr as NotebookAttr);
    let mut input = parse_macro_input!(item as ItemFn);
    let gorbie = gorbie_path();
    let original_ident = input.sig.ident.clone();
    let body_ident = Ident::new(
        &format!("__gorbie_{}_body", original_ident),
        Span::call_site(),
    );
    input.sig.ident = body_ident.clone();
    let vis = input.vis.clone();

    let mut setup_stmts: Vec<syn::Stmt> = Vec::new();
    if let Some(name) = name {
        setup_stmts.push(syn::parse_quote!(
            let mut __gorbie_notebook_owner = #gorbie::NotebookConfig::new(#name);
        ));
    } else {
        setup_stmts.push(syn::parse_quote!(let __gorbie_notebook_file = file!();));
        setup_stmts.push(syn::parse_quote!(
            let __gorbie_notebook_name = std::path::Path::new(__gorbie_notebook_file)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(__gorbie_notebook_file);
        ));
        setup_stmts.push(syn::parse_quote!(
            let mut __gorbie_notebook_owner =
                #gorbie::NotebookConfig::new(__gorbie_notebook_name);
        ));
    }

    let wrapper = quote! {
        #vis fn #original_ident() {
            #(#setup_stmts)*
            let mut __gorbie_headless = false;
            let mut __gorbie_headless_out_dir: Option<std::path::PathBuf> = None;
            let mut __gorbie_headless_scale: Option<f32> = None;
            let mut __gorbie_headless_wait_ms: Option<u64> = None;
            let mut __gorbie_args = std::env::args().skip(1);
            while let Some(arg) = __gorbie_args.next() {
                match arg.as_str() {
                    "--headless" => {
                        __gorbie_headless = true;
                    }
                    "--out-dir" => {
                        if let Some(dir) = __gorbie_args.next() {
                            __gorbie_headless_out_dir = Some(std::path::PathBuf::from(dir));
                        } else {
                            eprintln!("--out-dir expects a path");
                            return;
                        }
                    }
                    "--scale" | "--pixels-per-point" => {
                        if let Some(scale) = __gorbie_args.next() {
                            match scale.parse::<f32>() {
                                Ok(value) if value > 0.0 => {
                                    __gorbie_headless_scale = Some(value);
                                }
                                _ => {
                                    eprintln!("--scale expects a positive number");
                                    return;
                                }
                            }
                        } else {
                            eprintln!("--scale expects a number");
                            return;
                        }
                    }
                    "--headless-wait-ms" => {
                        if let Some(wait_ms) = __gorbie_args.next() {
                            match wait_ms.parse::<u64>() {
                                Ok(value) => {
                                    __gorbie_headless_wait_ms = Some(value);
                                }
                                _ => {
                                    eprintln!("--headless-wait-ms expects a non-negative integer");
                                    return;
                                }
                            }
                        } else {
                            eprintln!("--headless-wait-ms expects a number");
                            return;
                        }
                    }
                    _ => {}
                }
            }
            if __gorbie_headless {
                let out_dir = __gorbie_headless_out_dir
                    .unwrap_or_else(|| std::path::PathBuf::from("gorbie_capture"));
                __gorbie_notebook_owner = if let Some(scale) = __gorbie_headless_scale {
                    __gorbie_notebook_owner.with_headless_capture_scaled(out_dir, scale)
                } else {
                    __gorbie_notebook_owner.with_headless_capture(out_dir)
                };
                if let Some(wait_ms) = __gorbie_headless_wait_ms {
                    __gorbie_notebook_owner = __gorbie_notebook_owner
                        .with_headless_settle_timeout(std::time::Duration::from_millis(wait_ms));
                }
            }
            __gorbie_notebook_owner
                .run(|__gorbie_notebook_ctx| {
                    #body_ident(__gorbie_notebook_ctx);
                })
                .unwrap();
        }
    };

    TokenStream::from(quote! {
        #input
        #wrapper
    })
}

fn gorbie_path() -> proc_macro2::TokenStream {
    match crate_name("GORBIE") {
        Ok(FoundCrate::Itself) => {
            if is_library_crate() {
                quote!(crate)
            } else {
                let ident = Ident::new(&package_name(), Span::call_site());
                quote!(::#ident)
            }
        }
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name.replace('-', "_"), Span::call_site());
            quote!(::#ident)
        }
        Err(_) => {
            let ident = Ident::new(&package_name(), Span::call_site());
            quote!(::#ident)
        }
    }
}

fn is_library_crate() -> bool {
    let crate_name = std::env::var("CARGO_CRATE_NAME").ok();
    let package_name = std::env::var("CARGO_PKG_NAME").ok();
    crate_name.is_some() && crate_name == package_name
}

fn package_name() -> String {
    std::env::var("CARGO_PKG_NAME")
        .unwrap_or_else(|_| "GORBIE".to_owned())
        .replace('-', "_")
}
