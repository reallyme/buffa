//! Caller input policies for the cleanup visitor, separate from field layout.
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parser as _, Meta, Path};

/// Why a caller attribute cannot be preserved by the cleanup JSON visitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ErrorReason {
    /// The visitor cannot implement the requested input customization.
    #[error("unsupported caller JSON input attribute on a cleanup-enabled message")]
    Unsupported,
    /// A Serde default is neither bare nor a string containing a function path.
    #[error("invalid caller JSON default on a cleanup-enabled message")]
    InvalidDefault,
    /// Attribute syntax could not be parsed without guessing its policy.
    #[error("invalid caller JSON attribute syntax on a cleanup-enabled message")]
    InvalidSyntax,
}

#[derive(Default)]
pub(crate) struct Policy {
    skips: Vec<Vec<Meta>>,
    pub(crate) defaults: Vec<DefaultRule>,
}
pub(crate) struct DefaultRule {
    conditions: Vec<Meta>,
    path: Option<Path>,
}
impl DefaultRule {
    pub(crate) fn expression(&self) -> TokenStream {
        match &self.path {
            Some(path) => quote! { #path() },
            None => quote! { ::core::default::Default::default() },
        }
    }
    pub(crate) fn gate(&self, tokens: TokenStream) -> TokenStream {
        gate(condition(&self.conditions), tokens)
    }
}
fn condition(conditions: &[Meta]) -> Option<TokenStream> {
    (!conditions.is_empty()).then(|| quote! { all(#(#conditions),*) })
}
pub(crate) fn gate(condition: Option<TokenStream>, tokens: TokenStream) -> TokenStream {
    match condition {
        Some(condition) => quote! { #[cfg(#condition)] #tokens },
        None => tokens,
    }
}
impl Policy {
    pub(crate) fn acceptance(&self) -> Option<TokenStream> {
        if self.skips.is_empty() {
            return None;
        }
        let skips = self.skips.iter().map(|parts| {
            let condition = condition(parts);
            condition.unwrap_or_else(|| quote! { all() })
        });
        Some(quote! { not(any(#(#skips),*)) })
    }
}
fn options(
    list: &syn::MetaList,
) -> Result<syn::punctuated::Punctuated<Meta, syn::Token![,]>, ErrorReason> {
    list.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        .map_err(|_| ErrorReason::InvalidSyntax)
}
fn collect(
    meta: &Meta,
    conditions: &[Meta],
    policy: &mut Policy,
    container: bool,
) -> Result<(), ErrorReason> {
    let Meta::List(list) = meta else {
        return Ok(());
    };
    if list.path.is_ident("cfg_attr") {
        let mut nested = options(list)?.into_iter();
        let predicate = nested.next().ok_or(ErrorReason::InvalidSyntax)?;
        let mut conditions = conditions.to_vec();
        conditions.push(predicate);
        for meta in nested {
            collect(&meta, &conditions, policy, container)?;
        }
    } else if list.path.is_ident("cfg") {
        // Removing a field also affects binary/text/reflection implementations.
        // It cannot be treated as a JSON-only input policy.
        return Err(ErrorReason::Unsupported);
    } else if list.path.is_ident("serde") {
        for option in options(list)? {
            if option.path().is_ident("skip_serializing")
                || option.path().is_ident("skip_serializing_if")
                || option.path().is_ident("serialize_with")
            {
                continue;
            }
            if container {
                return Err(ErrorReason::Unsupported);
            }
            if option.path().is_ident("skip") || option.path().is_ident("skip_deserializing") {
                if !matches!(option, Meta::Path(_)) {
                    return Err(ErrorReason::InvalidSyntax);
                }
                policy.skips.push(conditions.to_vec());
            } else if option.path().is_ident("default") {
                let path = match option {
                    Meta::Path(_) => None,
                    Meta::NameValue(value) => {
                        let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(path),
                            ..
                        }) = value.value
                        else {
                            return Err(ErrorReason::InvalidDefault);
                        };
                        Some(
                            path.parse::<Path>()
                                .map_err(|_| ErrorReason::InvalidDefault)?,
                        )
                    }
                    _ => return Err(ErrorReason::InvalidDefault),
                };
                policy.defaults.push(DefaultRule {
                    conditions: conditions.to_vec(),
                    path,
                });
            } else {
                // Custom decoders, aliases and flattening must not appear to
                // work merely because Serialize accepts their attributes.
                return Err(ErrorReason::Unsupported);
            }
        }
    }
    Ok(())
}
pub(crate) fn parse(attributes: &TokenStream, container: bool) -> Result<Policy, ErrorReason> {
    let mut policy = Policy::default();
    for attribute in syn::Attribute::parse_outer
        .parse2(attributes.clone())
        .map_err(|_| ErrorReason::InvalidSyntax)?
    {
        collect(&attribute.meta, &[], &mut policy, container)?;
    }
    Ok(policy)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_skips_retain_nested_conditions() {
        let policy = parse(
            &quote! {
                #[serde(default)]
                #[cfg_attr(feature = "selected", cfg_attr(all(), serde(skip_deserializing)))]
            },
            false,
        )
        .unwrap();
        assert_eq!(policy.defaults.len(), 1);
        assert_eq!(
            policy.defaults[0].expression().to_string(),
            quote! { ::core::default::Default::default() }.to_string()
        );
        assert_eq!(policy.skips[0].len(), 2);
    }
    #[test]
    fn malformed_defaults_have_typed_redacted_errors() {
        for attribute in [
            quote! { #[serde(default = 17)] },
            quote! { #[serde(default = "private path!")] },
        ] {
            assert!(matches!(
                parse(&attribute, false),
                Err(ErrorReason::InvalidDefault)
            ));
        }
        assert!(matches!(
            parse(&quote! { #[cfg(any())] }, false),
            Err(ErrorReason::Unsupported)
        ));
    }
}
