use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::{quote, ToTokens};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{parse_macro_input, Expr, Ident, LitStr, Result, Token};

struct Dependencies {
    idents: Punctuated<Ident, Token![,]>,
}

impl Parse for Dependencies {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let idents = content.parse_terminated(Ident::parse, Token![,])?;
        Ok(Self { idents })
    }
}

struct ViewInput {
    notebook: Expr,
    dependencies: Dependencies,
    code: Expr,
}

impl Parse for ViewInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let notebook: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let dependencies: Dependencies = input.parse()?;
        input.parse::<Token![,]>()?;
        let code: Expr = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens"));
        }
        Ok(Self {
            notebook,
            dependencies,
            code,
        })
    }
}

struct StateInput {
    notebook: Expr,
    dependencies: Dependencies,
    init: Option<Expr>,
    code: Expr,
}

impl Parse for StateInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let notebook: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let dependencies: Dependencies = input.parse()?;
        input.parse::<Token![,]>()?;
        let first: Expr = input.parse()?;

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                return Ok(Self {
                    notebook,
                    dependencies,
                    init: None,
                    code: first,
                });
            }

            let second: Expr = input.parse()?;
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
            if !input.is_empty() {
                return Err(input.error("unexpected tokens"));
            }
            Ok(Self {
                notebook,
                dependencies,
                init: Some(first),
                code: second,
            })
        } else {
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
            if !input.is_empty() {
                return Err(input.error("unexpected tokens"));
            }
            Ok(Self {
                notebook,
                dependencies,
                init: None,
                code: first,
            })
        }
    }
}

struct DeriveInput {
    notebook: Expr,
    dependencies: Dependencies,
    code: Expr,
}

impl Parse for DeriveInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let notebook: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let dependencies: Dependencies = input.parse()?;
        input.parse::<Token![,]>()?;
        let code: Expr = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens"));
        }
        Ok(Self {
            notebook,
            dependencies,
            code,
        })
    }
}

#[proc_macro]
pub fn view(input: TokenStream) -> TokenStream {

    let mut s = input.clone().into_iter().map(|t| {
        t.span().source_text().unwrap_or("".to_string())
    }).fold(String::from("view!("), |mut acc, x| { acc.push_str(&x); acc });
    s.push_str(")");
    let input = parse_macro_input!(input as ViewInput);
    let gorbie = gorbie_path();
    let ViewInput {
        notebook,
        dependencies,
        code,
    } = input;
    let code_text = LitStr::new(&s, Span::call_site());
    let clones = dependency_clones(&dependencies);

    TokenStream::from(quote!({
        #(#clones)*
        #gorbie::cards::stateless_card(#notebook, #code, Some(#code_text))
    }))
}

#[proc_macro]
pub fn state(input: TokenStream) -> TokenStream {
    let mut s = input.clone().into_iter().map(|t| {
        t.span().source_text().unwrap_or("".to_string())
    }).fold(String::from("state!("), |mut acc, x| { acc.push_str(&x); acc });
    s.push_str(")");
    let input = parse_macro_input!(input as StateInput);
    let gorbie = gorbie_path();
    let StateInput {
        notebook,
        dependencies,
        init,
        code,
    } = input;
    let code_text = LitStr::new(&s, Span::call_site());
    let clones = dependency_clones(&dependencies);
    let init_expr = init.map_or_else(|| quote!(Default::default()), |expr| quote!(#expr));

    TokenStream::from(quote!({
        #(#clones)*
        #gorbie::cards::stateful_card(#notebook, #init_expr, #code, Some(#code_text))
    }))
}

#[proc_macro]
pub fn derive(input: TokenStream) -> TokenStream {
    let mut s = input.clone().into_iter().map(|t| {
        t.span().source_text().unwrap_or("".to_string())
    }).fold(String::from("derive!("), |mut acc, x| { acc.push_str(&x); acc });
    s.push_str(")");
    let input = parse_macro_input!(input as DeriveInput);
    let gorbie = gorbie_path();
    let DeriveInput {
        notebook,
        dependencies,
        code,
    } = input;
    let code_text = LitStr::new(&s, Span::call_site());
    let clones = dependency_clones(&dependencies);
    let dep_idents = dependencies.idents.iter();

    TokenStream::from(quote!({
        #(#clones)*
        #gorbie::cards::reactive_card(#notebook, (#(#dep_idents),*,), #code, Some(#code_text))
    }))
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

fn dependency_clones(dependencies: &Dependencies) -> Vec<proc_macro2::TokenStream> {
    dependencies
        .idents
        .iter()
        .map(|ident| quote!(let #ident = #ident.clone();))
        .collect()
}

fn code_literal(expr: &Expr) -> LitStr {
    let token_text = expr.to_token_stream().to_string();
    let mut text = expr.span().source_text().unwrap_or_else(|| token_text.clone());
    let source_tokens = text.split_whitespace().count();
    let token_tokens = token_text.split_whitespace().count();
    if source_tokens < token_tokens {
        text = token_text;
    }
    LitStr::new(&text, Span::call_site())
}
