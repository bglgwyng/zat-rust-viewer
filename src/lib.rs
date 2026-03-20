use std::collections::HashSet;
use syn::{
    visit::Visit, Attribute, FnArg, GenericParam, Generics, Item, ItemConst, ItemEnum,
    ItemFn, ItemMod, ItemStatic, ItemStruct, ItemTrait, ItemType, ItemUse, Pat, ReturnType,
    Signature, TraitItem, Type, TypePath, Visibility,
};

pub struct OutlineEntry {
    pub signature: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct ImportEntry {
    pub source_text: String,
    pub start_line: usize,
    pub end_line: usize,
}

pub struct OutlineResult {
    pub imports: Vec<ImportEntry>,
    pub exports: Vec<OutlineEntry>,
}

pub fn extract_outline(source: &str) -> OutlineResult {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => {
            return OutlineResult {
                imports: Vec::new(),
                exports: Vec::new(),
            }
        }
    };

    let mut exports = Vec::new();
    let mut type_refs: HashSet<String> = HashSet::new();
    let mut use_items: Vec<&ItemUse> = Vec::new();

    for item in &file.items {
        match item {
            Item::Use(item_use) => {
                // Collect pub use as exports, all use for potential import filtering
                if is_pub(&item_use.vis) {
                    let text = use_tree_to_string(&item_use.tree);
                    let (start, end) = span_lines(source, item_use);
                    exports.push(OutlineEntry {
                        signature: format!("use {}", text),
                        start_line: start,
                        end_line: end,
                    });
                } else {
                    use_items.push(item_use);
                }
            }
            Item::Fn(item_fn) => {
                if is_pub(&item_fn.vis) {
                    let sig = format_fn_signature(item_fn, &mut type_refs);
                    let (start, end) = span_lines(source, item_fn);
                    exports.push(OutlineEntry {
                        signature: sig,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Item::Struct(item_struct) => {
                if is_pub(&item_struct.vis) {
                    let mut entries = format_struct_entries(source, item_struct, &mut type_refs);
                    exports.append(&mut entries);
                }
            }
            Item::Enum(item_enum) => {
                if is_pub(&item_enum.vis) {
                    let mut entries = format_enum_entries(source, item_enum, &mut type_refs);
                    exports.append(&mut entries);
                }
            }
            Item::Trait(item_trait) => {
                if is_pub(&item_trait.vis) {
                    let sig = format_trait_signature(item_trait, &mut type_refs);
                    let (start, end) = span_lines(source, item_trait);
                    exports.push(OutlineEntry {
                        signature: sig,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Item::Type(item_type) => {
                if is_pub(&item_type.vis) {
                    let sig = format_type_alias_signature(item_type, &mut type_refs);
                    let (start, end) = span_lines(source, item_type);
                    exports.push(OutlineEntry {
                        signature: sig,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Item::Const(item_const) => {
                if is_pub(&item_const.vis) {
                    let sig = format_const_signature(item_const, &mut type_refs);
                    let (start, end) = span_lines(source, item_const);
                    exports.push(OutlineEntry {
                        signature: sig,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Item::Static(item_static) => {
                if is_pub(&item_static.vis) {
                    let sig = format_static_signature(item_static, &mut type_refs);
                    let (start, end) = span_lines(source, item_static);
                    exports.push(OutlineEntry {
                        signature: sig,
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            Item::Mod(item_mod) => {
                if is_pub(&item_mod.vis) {
                    let (start, end) = span_lines(source, item_mod);
                    exports.push(OutlineEntry {
                        signature: format_mod_signature(item_mod),
                        start_line: start,
                        end_line: end,
                    });
                }
            }
            _ => {}
        }
    }

    // Filter use statements: only keep those whose imported names appear in type_refs
    let imports = use_items
        .into_iter()
        .filter(|item_use| {
            let names = collect_use_names(&item_use.tree);
            names.iter().any(|n| type_refs.contains(n))
        })
        .map(|item_use| {
            let (start, end) = span_lines(source, item_use);
            let text = source_text_for_span(source, item_use);
            ImportEntry {
                source_text: text,
                start_line: start,
                end_line: end,
            }
        })
        .collect();

    OutlineResult { imports, exports }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn span_lines<T: syn::spanned::Spanned>(source: &str, item: &T) -> (usize, usize) {
    let span = item.span();
    let start = span.start().line;
    let end = span.end().line;
    // proc-macro2 span locations are 1-based when CARGO_MANIFEST_DIR is set
    // but we need to handle the case where they might be 0
    if start == 0 && end == 0 {
        // Fallback: count newlines from byte offset if span locations aren't available
        // This shouldn't happen with proc-macro2 span-locations feature
        return (1, source.lines().count());
    }
    (start, end)
}

fn source_text_for_span<T: syn::spanned::Spanned>(source: &str, item: &T) -> String {
    let span = item.span();
    let start_line = span.start().line;
    let end_line = span.end().line;
    if start_line == 0 {
        return String::new();
    }
    let lines: Vec<&str> = source.lines().collect();
    let start_idx = start_line.saturating_sub(1);
    let end_idx = end_line.min(lines.len());
    lines[start_idx..end_idx].join("\n")
}

fn format_fn_signature(item_fn: &ItemFn, type_refs: &mut HashSet<String>) -> String {
    format_sig(&item_fn.sig, type_refs)
}

fn format_sig(sig: &Signature, type_refs: &mut HashSet<String>) -> String {
    let async_prefix = if sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let unsafe_prefix = if sig.unsafety.is_some() {
        "unsafe "
    } else {
        ""
    };
    let name = &sig.ident;
    let generics = format_generics(&sig.generics, type_refs);
    let params = format_fn_params(sig, type_refs);
    let ret = format_return_type(&sig.output, type_refs);
    let where_clause = format_where_clause(&sig.generics, type_refs);

    format!(
        "{}{}fn {}{}({}){}{}",
        async_prefix, unsafe_prefix, name, generics, params, ret, where_clause
    )
}

fn format_generics(generics: &Generics, type_refs: &mut HashSet<String>) -> String {
    if generics.params.is_empty() {
        return String::new();
    }
    let params: Vec<String> = generics
        .params
        .iter()
        .map(|p| match p {
            GenericParam::Type(tp) => {
                let name = tp.ident.to_string();
                if tp.bounds.is_empty() {
                    name
                } else {
                    let bounds: Vec<String> = tp
                        .bounds
                        .iter()
                        .map(|b| {
                            let s = quote::quote!(#b).to_string();
                            collect_type_refs_from_str(&s, type_refs);
                            s
                        })
                        .collect();
                    format!("{}: {}", name, bounds.join(" + "))
                }
            }
            GenericParam::Lifetime(lt) => format!("'{}", lt.lifetime.ident),
            GenericParam::Const(c) => {
                let ty_str = quote::quote!(#c.ty).to_string();
                format!("const {}: {}", c.ident, ty_str)
            }
        })
        .collect();
    format!("<{}>", params.join(", "))
}

fn format_where_clause(generics: &Generics, type_refs: &mut HashSet<String>) -> String {
    match &generics.where_clause {
        Some(wc) => {
            let predicates: Vec<String> = wc
                .predicates
                .iter()
                .map(|p| {
                    let s = quote::quote!(#p).to_string();
                    collect_type_refs_from_str(&s, type_refs);
                    s
                })
                .collect();
            format!(" where {}", predicates.join(", "))
        }
        None => String::new(),
    }
}

fn format_fn_params(sig: &Signature, type_refs: &mut HashSet<String>) -> String {
    sig.inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Receiver(r) => {
                let ref_tok = if r.reference.is_some() { "&" } else { "" };
                let lt = r
                    .reference
                    .as_ref()
                    .and_then(|(_and, lt)| lt.as_ref())
                    .map(|lt| format!("'{} ", lt.ident))
                    .unwrap_or_default();
                let mutability = if r.mutability.is_some() {
                    "mut "
                } else {
                    ""
                };
                format!("{}{}{}self", ref_tok, lt, mutability)
            }
            FnArg::Typed(pat_type) => {
                let name = match pat_type.pat.as_ref() {
                    Pat::Ident(pi) => pi.ident.to_string(),
                    Pat::Wild(_) => "_".to_string(),
                    other => quote::quote!(#other).to_string(),
                };
                let ty = format_type(&pat_type.ty, type_refs);
                format!("{}: {}", name, ty)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_return_type(ret: &ReturnType, type_refs: &mut HashSet<String>) -> String {
    match ret {
        ReturnType::Default => String::new(),
        ReturnType::Type(_, ty) => {
            let ty_str = format_type(ty, type_refs);
            format!(" -> {}", ty_str)
        }
    }
}

fn format_type(ty: &Type, type_refs: &mut HashSet<String>) -> String {
    // Use a visitor to collect type references, then produce the string
    let mut collector = TypeRefCollector {
        refs: type_refs,
    };
    collector.visit_type(ty);

    let s = quote::quote!(#ty).to_string();
    s
}

struct TypeRefCollector<'a> {
    refs: &'a mut HashSet<String>,
}

impl<'a> Visit<'_> for TypeRefCollector<'a> {
    fn visit_type_path(&mut self, node: &TypePath) {
        if let Some(ident) = node.path.get_ident() {
            self.refs.insert(ident.to_string());
        } else if let Some(first) = node.path.segments.first() {
            self.refs.insert(first.ident.to_string());
        }
        syn::visit::visit_type_path(self, node);
    }
}

fn collect_type_refs_from_str(s: &str, type_refs: &mut HashSet<String>) {
    // Simple heuristic: extract identifiers that look like type names (start with uppercase)
    for word in s.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if !word.is_empty() && word.chars().next().map_or(false, |c| c.is_uppercase()) {
            type_refs.insert(word.to_string());
        }
    }
}

fn format_struct_entries(source: &str, item_struct: &ItemStruct, type_refs: &mut HashSet<String>) -> Vec<OutlineEntry> {
    let derives = extract_derives(&item_struct.attrs);
    let derive_str = if derives.is_empty() {
        String::new()
    } else {
        format!("#[derive({})]\n", derives.join(", "))
    };

    let (start, end) = span_lines(source, item_struct);

    // Collect pub fields
    let mut field_entries = Vec::new();
    if let syn::Fields::Named(ref fields) = item_struct.fields {
        for field in &fields.named {
            if is_pub(&field.vis) {
                if let Some(ref ident) = field.ident {
                    let ty = format_type(&field.ty, type_refs);
                    let (f_start, f_end) = span_lines(source, field);
                    field_entries.push(OutlineEntry {
                        signature: format!("  {}: {}", ident, ty),
                        start_line: f_start,
                        end_line: f_end,
                    });
                }
            }
        }
    }

    let mut entries = Vec::new();
    if field_entries.is_empty() {
        entries.push(OutlineEntry {
            signature: format!("{}struct {}", derive_str, item_struct.ident),
            start_line: start,
            end_line: end,
        });
    } else {
        entries.push(OutlineEntry {
            signature: format!("{}struct {} {{", derive_str, item_struct.ident),
            start_line: start,
            end_line: end,
        });
        entries.append(&mut field_entries);
        entries.push(OutlineEntry {
            signature: "}".to_string(),
            start_line: 0,
            end_line: 0,
        });
    }
    entries
}

fn format_enum_entries(source: &str, item_enum: &ItemEnum, type_refs: &mut HashSet<String>) -> Vec<OutlineEntry> {
    let derives = extract_derives(&item_enum.attrs);
    let derive_str = if derives.is_empty() {
        String::new()
    } else {
        format!("#[derive({})]\n", derives.join(", "))
    };
    let (start, end) = span_lines(source, item_enum);

    let mut entries = Vec::new();
    entries.push(OutlineEntry {
        signature: format!("{}enum {} {{", derive_str, item_enum.ident),
        start_line: start,
        end_line: end,
    });

    for variant in &item_enum.variants {
        let name = &variant.ident;
        let fields_str = match &variant.fields {
            syn::Fields::Unit => String::new(),
            syn::Fields::Unnamed(fields) => {
                let types: Vec<String> = fields.unnamed.iter()
                    .map(|f| format_type(&f.ty, type_refs))
                    .collect();
                format!("({})", types.join(", "))
            }
            syn::Fields::Named(fields) => {
                let members: Vec<String> = fields.named.iter()
                    .map(|f| {
                        let ty = format_type(&f.ty, type_refs);
                        format!("{}: {}", f.ident.as_ref().unwrap(), ty)
                    })
                    .collect();
                format!(" {{ {} }}", members.join(", "))
            }
        };
        entries.push(OutlineEntry {
            signature: format!("  {}{}", name, fields_str),
            start_line: 0,
            end_line: 0,
        });
    }

    entries.push(OutlineEntry {
        signature: "}".to_string(),
        start_line: 0,
        end_line: 0,
    });

    entries
}

fn format_trait_signature(
    item_trait: &ItemTrait,
    type_refs: &mut HashSet<String>,
) -> String {
    let name = &item_trait.ident;
    let generics = format_generics(&item_trait.generics, type_refs);
    let where_clause = format_where_clause(&item_trait.generics, type_refs);

    let mut methods = Vec::new();
    for trait_item in &item_trait.items {
        if let TraitItem::Fn(method) = trait_item {
            let sig = format_sig(&method.sig, type_refs);
            methods.push(sig);
        }
    }

    if methods.is_empty() {
        format!("trait {}{}{}", name, generics, where_clause)
    } else {
        format!(
            "trait {}{}{} {{\n{}\n}}",
            name,
            generics,
            where_clause,
            methods
                .iter()
                .map(|m| format!("  {};", m))
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

fn format_type_alias_signature(
    item_type: &ItemType,
    type_refs: &mut HashSet<String>,
) -> String {
    let name = &item_type.ident;
    let generics = format_generics(&item_type.generics, type_refs);
    format!("type {}{}", name, generics)
}

fn format_const_signature(
    item_const: &ItemConst,
    type_refs: &mut HashSet<String>,
) -> String {
    let name = &item_const.ident;
    let ty = format_type(&item_const.ty, type_refs);
    format!("const {}: {}", name, ty)
}

fn format_static_signature(
    item_static: &ItemStatic,
    type_refs: &mut HashSet<String>,
) -> String {
    let name = &item_static.ident;
    let mutability = match item_static.mutability {
        syn::StaticMutability::Mut(_) => "mut ",
        _ => "",
    };
    let ty = format_type(&item_static.ty, type_refs);
    format!("static {}{}: {}", mutability, name, ty)
}

fn format_mod_signature(item_mod: &ItemMod) -> String {
    format!("mod {}", item_mod.ident)
}

fn extract_derives(attrs: &[Attribute]) -> Vec<String> {
    let mut derives = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if let Ok(nested) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) {
                for path in nested {
                    derives.push(
                        path.segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect::<Vec<_>>()
                            .join("::"),
                    );
                }
            }
        }
    }
    derives
}

fn use_tree_to_string(tree: &syn::UseTree) -> String {
    quote::quote!(#tree).to_string()
}

fn collect_use_names(tree: &syn::UseTree) -> Vec<String> {
    let mut names = Vec::new();
    collect_use_names_inner(tree, &mut names);
    names
}

fn collect_use_names_inner(tree: &syn::UseTree, names: &mut Vec<String>) {
    match tree {
        syn::UseTree::Path(path) => {
            collect_use_names_inner(&path.tree, names);
        }
        syn::UseTree::Name(name) => {
            names.push(name.ident.to_string());
        }
        syn::UseTree::Rename(rename) => {
            names.push(rename.rename.to_string());
        }
        syn::UseTree::Glob(_) => {
            // Glob imports can't be filtered meaningfully
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_use_names_inner(item, names);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pub_function() {
        let source = r#"
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert_eq!(
            result.exports[0].signature,
            "fn greet(name: String) -> String"
        );
        assert_eq!(result.exports[0].start_line, 2);
        assert_eq!(result.exports[0].end_line, 4);
    }

    #[test]
    fn test_private_function_excluded() {
        let source = r#"
fn private_helper() -> i32 {
    42
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 0);
    }

    #[test]
    fn test_pub_struct_with_derive() {
        let source = r#"
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub age: u32,
}
"#;
        let result = extract_outline(source);
        // header + 2 pub fields + closing brace = 4 entries
        assert_eq!(result.exports.len(), 4);
        assert!(result.exports[0].signature.contains("struct User"));
        assert!(result.exports[0].signature.contains("Debug"));
        assert!(result.exports[0].signature.contains("Clone"));
        assert!(result.exports[1].signature.contains("name: String"));
        assert!(result.exports[2].signature.contains("age: u32"));
    }

    #[test]
    fn test_pub_enum() {
        let source = r#"
#[derive(Debug)]
pub enum Color {
    Red,
    Green,
    Blue,
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 5);
        assert!(result.exports[0].signature.contains("#[derive(Debug)]"));
        assert!(result.exports[0].signature.contains("enum Color {"));
        assert_eq!(result.exports[1].signature, "  Red");
        assert_eq!(result.exports[2].signature, "  Green");
        assert_eq!(result.exports[3].signature, "  Blue");
        assert_eq!(result.exports[4].signature, "}");
    }

    #[test]
    fn test_pub_trait_with_methods() {
        let source = r#"
pub trait Processor {
    fn process(&self, input: Vec<u8>) -> Result<String, Error>;
    fn reset(&mut self);
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert!(result.exports[0].signature.contains("trait Processor"));
        assert!(result.exports[0].signature.contains("fn process"));
        assert!(result.exports[0].signature.contains("fn reset"));
    }

    #[test]
    fn test_pub_type_alias() {
        let source = r#"
pub type Result<T> = std::result::Result<T, MyError>;
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert!(result.exports[0].signature.contains("type Result"));
    }

    #[test]
    fn test_pub_const() {
        let source = r#"
pub const MAX_SIZE: usize = 1024;
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert_eq!(
            result.exports[0].signature,
            "const MAX_SIZE: usize"
        );
    }

    #[test]
    fn test_pub_static() {
        let source = r#"
pub static GLOBAL_COUNT: u32 = 0;
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert_eq!(
            result.exports[0].signature,
            "static GLOBAL_COUNT: u32"
        );
    }

    #[test]
    fn test_pub_mod() {
        let source = r#"
pub mod utils;
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert_eq!(result.exports[0].signature, "mod utils");
    }

    #[test]
    fn test_pub_use_as_export() {
        let source = r#"
pub use crate::types::Config;
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert!(result.exports[0].signature.contains("use"));
        assert!(result.exports[0].signature.contains("Config"));
    }

    #[test]
    fn test_async_function() {
        let source = r#"
pub async fn fetch_data(url: String) -> Result<Vec<u8>, Error> {
    todo!()
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert!(result.exports[0].signature.contains("async"));
        assert!(result.exports[0].signature.contains("fetch_data"));
    }

    #[test]
    fn test_import_filtering() {
        let source = r#"
use std::collections::HashMap;
use std::io::Read;

pub fn get_map() -> HashMap<String, i32> {
    HashMap::new()
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        // HashMap is referenced in the signature, Read is not
        assert_eq!(result.imports.len(), 1);
        assert!(result.imports[0].source_text.contains("HashMap"));
    }

    #[test]
    fn test_import_filtering_none_referenced() {
        let source = r#"
use std::io::Read;

pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 1);
        assert_eq!(result.imports.len(), 0);
    }

    #[test]
    fn test_line_numbers() {
        let source = r#"use std::io;

pub fn first() -> i32 {
    // line 4
    // line 5
    42
}

pub fn second() -> String {
    // line 10
    String::new()
}
"#;
        let result = extract_outline(source);
        assert_eq!(result.exports.len(), 2);
        assert_eq!(result.exports[0].start_line, 3);
        assert_eq!(result.exports[0].end_line, 7);
        assert_eq!(result.exports[1].start_line, 9);
        assert_eq!(result.exports[1].end_line, 12);
    }
}
