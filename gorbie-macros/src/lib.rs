use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Expr, Ident, LitStr, Result, Token};

struct Dependencies {
    exprs: Punctuated<Expr, Token![,]>,
}

impl Parse for Dependencies {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        syn::bracketed!(content in input);
        let exprs = content.parse_terminated(Expr::parse, Token![,])?;
        Ok(Self { exprs })
    }
}

struct ViewInput {
    notebook: Expr,
    code: Expr,
}

impl Parse for ViewInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let notebook: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let code: Expr = input.parse()?;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        }
        if !input.is_empty() {
            return Err(input.error("unexpected tokens"));
        }
        Ok(Self { notebook, code })
    }
}

struct StateInput {
    notebook: Expr,
    init: Option<Expr>,
    code: Expr,
}

impl Parse for StateInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let notebook: Expr = input.parse()?;
        input.parse::<Token![,]>()?;
        let first: Expr = input.parse()?;

        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                return Ok(Self {
                    notebook,
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
    let code_text = LitStr::new(&macro_source_text("view!", &input), Span::call_site());
    let input = parse_macro_input!(input as ViewInput);
    let gorbie = gorbie_path();
    let ViewInput { notebook, code } = input;

    TokenStream::from(quote!({
        #gorbie::cards::stateless_card(#notebook, #code, Some(#code_text))
    }))
}

#[proc_macro]
pub fn state(input: TokenStream) -> TokenStream {
    let code_text = LitStr::new(&macro_source_text("state!", &input), Span::call_site());
    let input = parse_macro_input!(input as StateInput);
    let gorbie = gorbie_path();
    let StateInput {
        notebook,
        init,
        code,
    } = input;
    let init_expr = init.map_or_else(|| quote!(Default::default()), |expr| quote!(#expr));

    TokenStream::from(quote!({
        #gorbie::cards::stateful_card(#notebook, #init_expr, #code, Some(#code_text))
    }))
}

#[proc_macro]
pub fn derive(input: TokenStream) -> TokenStream {
    let code_text = LitStr::new(&macro_source_text("derive!", &input), Span::call_site());
    let input = parse_macro_input!(input as DeriveInput);
    let gorbie = gorbie_path();
    let DeriveInput {
        notebook,
        dependencies,
        code,
    } = input;
    let dep_keys = dependencies
        .exprs
        .iter()
        .map(|expr| quote!(#gorbie::state::DependencyKey::new(#expr)));

    TokenStream::from(quote!({
        #gorbie::cards::reactive_card(#notebook, vec![#(#dep_keys),*], #code, Some(#code_text))
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

fn macro_source_text(name: &str, input: &TokenStream) -> String {
    let mut text = String::new();
    text.push_str(name);
    text.push('(');

    let mut iter = input.clone().into_iter().peekable();
    let mut prev = None;
    while let Some(token) = next_token(&mut iter) {
        if let Some(prev_token) = &prev {
            if needs_space(prev_token, &token) {
                text.push(' ');
            }
        }
        text.push_str(&token.text);
        prev = Some(token);
    }

    text.push(')');
    text
}

#[derive(Clone)]
enum TokenKind {
    IdentLike,
    Group(proc_macro::Delimiter),
    Punct,
}

#[derive(Clone)]
struct TokenInfo {
    text: String,
    kind: TokenKind,
}

fn next_token<I>(iter: &mut std::iter::Peekable<I>) -> Option<TokenInfo>
where
    I: Iterator<Item = proc_macro::TokenTree>,
{
    let token = iter.next()?;
    let info = match token {
        proc_macro::TokenTree::Ident(ident) => TokenInfo {
            text: ident
                .span()
                .source_text()
                .unwrap_or_else(|| ident.to_string()),
            kind: TokenKind::IdentLike,
        },
        proc_macro::TokenTree::Literal(literal) => TokenInfo {
            text: literal
                .span()
                .source_text()
                .unwrap_or_else(|| literal.to_string()),
            kind: TokenKind::IdentLike,
        },
        proc_macro::TokenTree::Group(group) => TokenInfo {
            text: group
                .span()
                .source_text()
                .unwrap_or_else(|| group.to_string()),
            kind: TokenKind::Group(group.delimiter()),
        },
        proc_macro::TokenTree::Punct(punct) => {
            let mut text = punct
                .span()
                .source_text()
                .unwrap_or_else(|| punct.to_string());
            let mut spacing = punct.spacing();
            while spacing == proc_macro::Spacing::Joint {
                if !matches!(iter.peek(), Some(proc_macro::TokenTree::Punct(_))) {
                    break;
                }
                let proc_macro::TokenTree::Punct(next_punct) =
                    iter.next().expect("peeked punctuation should be present")
                else {
                    break;
                };
                text.push_str(
                    &next_punct
                        .span()
                        .source_text()
                        .unwrap_or_else(|| next_punct.to_string()),
                );
                spacing = next_punct.spacing();
            }
            TokenInfo {
                text,
                kind: TokenKind::Punct,
            }
        }
    };
    Some(info)
}

fn needs_space(prev: &TokenInfo, curr: &TokenInfo) -> bool {
    if curr.is_punct(",") || curr.is_punct(";") {
        return false;
    }
    if curr.is_punct("::") || curr.is_punct(".") || curr.is_punct("!") {
        return false;
    }
    if prev.is_punct("::") || prev.is_punct(".") || prev.is_punct("!") {
        return false;
    }
    if prev.is_ident_like() && curr.is_group_paren_or_bracket() {
        return false;
    }
    if curr.is_pipe() && prev.is_ident_like() {
        return false;
    }
    if prev.is_pipe() {
        if curr.is_group_brace() {
            return true;
        }
        if curr.is_ident_like() {
            return false;
        }
    }
    true
}

impl TokenInfo {
    fn is_ident_like(&self) -> bool {
        matches!(self.kind, TokenKind::IdentLike | TokenKind::Group(_))
    }

    fn is_group_paren_or_bracket(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Group(proc_macro::Delimiter::Parenthesis)
                | TokenKind::Group(proc_macro::Delimiter::Bracket)
        )
    }

    fn is_group_brace(&self) -> bool {
        matches!(self.kind, TokenKind::Group(proc_macro::Delimiter::Brace))
    }

    fn is_pipe(&self) -> bool {
        self.is_punct("|")
    }

    fn is_punct(&self, text: &str) -> bool {
        matches!(self.kind, TokenKind::Punct) && self.text == text
    }
}
