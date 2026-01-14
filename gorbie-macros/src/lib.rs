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
            let __gorbie_notebook_owner = #gorbie::Notebook::new(#name);
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
            let __gorbie_notebook_owner = #gorbie::Notebook::new(__gorbie_notebook_name);
        ));
    }

    let wrapper = quote! {
        #vis fn #original_ident() {
            #(#setup_stmts)*
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
