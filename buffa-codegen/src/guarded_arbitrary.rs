// Modified by ReallyMe Technologies Limited.
//! Construct guarded messages in an early cleanup owner, without derive scratch.

use crate::context::MessageScope;
use crate::generated::descriptor::field_descriptor_proto::{Label, Type};
use crate::generated::descriptor::DescriptorProto;
use crate::{impl_message, CodeGenError};
use proc_macro2::{Ident, TokenStream};
use quote::quote;
use std::collections::HashMap;

pub(crate) fn value(kind: Type, ty: &TokenStream) -> TokenStream {
    match kind {
        Type::TYPE_BYTES => quote! { ::buffa::sensitive::arbitrary::bytes::<#ty>(__u)? },
        Type::TYPE_STRING => quote! { ::buffa::sensitive::arbitrary::string::<#ty>(__u)? },
        Type::TYPE_MESSAGE | Type::TYPE_GROUP => quote! {
            <#ty as ::buffa::sensitive::GuardedArbitrary<'a>>::arbitrary_guarded(__u, __depth)?
        },
        _ => quote! { <#ty as ::arbitrary::Arbitrary<'a>>::arbitrary(__u)? },
    }
}

pub(crate) fn implementation(ty: &Ident, body: TokenStream, allow: &TokenStream) -> TokenStream {
    quote! {
        #[cfg(feature = "arbitrary")]
        #allow
        impl<'a> ::buffa::sensitive::GuardedArbitrary<'a> for #ty {
            // The owner must exist before a later field initializer can fail.
            #[allow(clippy::field_reassign_with_default)]
            fn arbitrary_guarded(
                __u: &mut ::arbitrary::Unstructured<'a>,
                __remaining_depth: u32,
            ) -> ::arbitrary::Result<Self> {
                let __depth = __remaining_depth.checked_sub(1)
                    .ok_or(::arbitrary::Error::NotEnoughData)?;
                let _ = __depth;
                #body
            }
        }
        #[cfg(feature = "arbitrary")]
        impl<'a> ::arbitrary::Arbitrary<'a> for #ty {
            fn arbitrary(__u: &mut ::arbitrary::Unstructured<'a>) -> ::arbitrary::Result<Self> {
                <Self as ::buffa::sensitive::GuardedArbitrary<'a>>::arbitrary_guarded(
                    __u, ::buffa::RECURSION_LIMIT,
                )
            }
        }
    }
}

pub(crate) fn message(
    scope: MessageScope<'_>,
    descriptor: &DescriptorProto,
    name: &Ident,
    oneofs: &HashMap<usize, Ident>,
    oneof_prefix: &TokenStream,
    resolver: &crate::imports::ImportResolver,
) -> Result<TokenStream, CodeGenError> {
    let mut fields = Vec::new();
    for field in &descriptor.field {
        if impl_message::is_real_oneof_member(field) {
            continue;
        }
        let field_name = field
            .name
            .as_deref()
            .ok_or(CodeGenError::MissingField("field.name"))?;
        let ident = scope.ctx.field_ident(field_name, field.number.unwrap_or(0));
        let kind = impl_message::effective_type(scope.ctx, field, scope.features);
        if let Some(entry) = crate::message::find_map_entry(descriptor, field) {
            let info = crate::message::classify_field(scope, descriptor, field, resolver)?;
            let collection = info.struct_field_type();
            let key = entry
                .field
                .iter()
                .find(|slot| slot.number == Some(1))
                .ok_or(CodeGenError::MissingField("map.key"))?;
            let val = entry
                .field
                .iter()
                .find(|slot| slot.number == Some(2))
                .ok_or(CodeGenError::MissingField("map.value"))?;
            let key_type = quote! { <#collection as ::buffa::map_codec::MapStorage>::Key };
            let val_type = quote! { <#collection as ::buffa::map_codec::MapStorage>::Value };
            let key_value = value(
                impl_message::effective_type_in_map_entry(scope.ctx, key, scope.features),
                &key_type,
            );
            let val_value = value(
                impl_message::effective_type_in_map_entry(scope.ctx, val, scope.features),
                &val_type,
            );
            fields.push(quote! {
                while !__u.is_empty() && <bool as ::arbitrary::Arbitrary<'a>>::arbitrary(__u)? {
                    let __key = ::buffa::sensitive::Slot::new(
                        #key_value, <#collection as ::buffa::map_codec::MapStorage>::storage_erase_key,
                    );
                    let __value = ::buffa::sensitive::Slot::new(
                        #val_value, <#collection as ::buffa::map_codec::MapStorage>::storage_erase_value,
                    );
                    ::buffa::map_codec::MapStorage::storage_insert(
                        &mut __owned.#ident, __key.into_inner(), __value.into_inner(),
                    );
                }
            });
        } else if field.label == Some(Label::LABEL_REPEATED) {
            let element = value(kind, &quote! { _ });
            fields.push(quote! {
                while !__u.is_empty() && <bool as ::arbitrary::Arbitrary<'a>>::arbitrary(__u)? {
                    ::buffa::ProtoList::push(&mut __owned.#ident, #element);
                }
            });
        } else if matches!(kind, Type::TYPE_MESSAGE | Type::TYPE_GROUP) {
            let inner = value(kind, &quote! { _ });
            let info = crate::message::classify_field(scope, descriptor, field, resolver)?;
            let ty = info.struct_field_type();
            fields.push(quote! {
                __owned.#ident = if <bool as ::arbitrary::Arbitrary<'a>>::arbitrary(__u)? {
                    <#ty>::some(#inner)
                } else { <#ty>::none() };
            });
        } else {
            let element = value(kind, &quote! { _ });
            let features = crate::features::resolve_field(scope.ctx, field, scope.features);
            let expression = if impl_message::is_explicit_presence_scalar(field, kind, &features) {
                quote! {
                    if <bool as ::arbitrary::Arbitrary<'a>>::arbitrary(__u)? {
                        Some(#element)
                    } else { None }
                }
            } else {
                element
            };
            fields.push(quote! { __owned.#ident = #expression; });
        }
    }
    for (index, oneof) in descriptor.oneof_decl.iter().enumerate() {
        let Some(enum_name) = oneofs.get(&index) else {
            continue;
        };
        let field = scope.ctx.oneof_ident(
            oneof
                .name
                .as_deref()
                .ok_or(CodeGenError::MissingField("oneof.name"))?,
        );
        fields.push(quote! {
            __owned.#field = if <bool as ::arbitrary::Arbitrary<'a>>::arbitrary(__u)? {
                Some(<#oneof_prefix #enum_name as ::buffa::sensitive::GuardedArbitrary<'a>>::arbitrary_guarded(__u, __depth)?)
            } else { None };
        });
    }
    let mutable = (!fields.is_empty()).then(|| quote! { mut });
    let body = quote! {
        let #mutable __owned = Self::default();
        #(#fields)*
        Ok(__owned)
    };
    let allow = crate::message::deprecated_field_allow(scope.ctx, descriptor, scope.proto_fqn);
    Ok(implementation(name, body, &allow))
}
