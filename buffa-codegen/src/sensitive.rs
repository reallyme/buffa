// Modified by ReallyMe Technologies Limited.
//! Recursive owned-message cleanup, enabled explicitly by the consumer.

use crate::context::{CodeGenContext, MessageScope};
use crate::generated::descriptor::{field_descriptor_proto::Type, DescriptorProto};
use crate::{BytesRepr, CodeGenError, StringRepr, ZeroizeConfigurationErrorReason as Reason};
use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(crate) fn message_impl(
    scope: MessageScope<'_>,
    msg: &DescriptorProto,
    name: &Ident,
    resolver: &crate::imports::ImportResolver,
) -> Result<TokenStream, CodeGenError> {
    let MessageScope {
        ctx,
        proto_fqn,
        features,
        ..
    } = scope;
    if !ctx.zeroizes_message(proto_fqn) {
        if ctx.contains_guarded_reflection(proto_fqn) {
            return Err(invalid(proto_fqn, Reason::ReflectionParent));
        }
        return Ok(quote! {});
    }
    if ctx.preserve_unknown_fields(proto_fqn) {
        return Err(invalid(proto_fqn, Reason::UnknownFieldStorage));
    }
    let mut fields = Vec::new();
    let mut guards = Vec::new();
    for field in &msg.field {
        let field_name = field
            .name
            .as_deref()
            .ok_or(CodeGenError::MissingField("field.name"))?;
        let ty = crate::impl_message::effective_type(ctx, field, features);
        let string_repr = crate::impl_message::field_string_repr(ctx, proto_fqn, field_name);
        let bytes_repr = crate::impl_message::field_bytes_repr(ctx, proto_fqn, field_name);
        let mut slot_types = vec![ty];
        if let Some(entry) = crate::message::find_map_entry(msg, field) {
            if crate::message::map_entry_key_type(ctx, entry, features) == Some(Type::TYPE_BYTES) {
                return Err(invalid(proto_fqn, Reason::RawMapKey));
            }
            slot_types = entry
                .field
                .iter()
                .map(|slot| crate::impl_message::effective_type_in_map_entry(ctx, slot, features))
                .collect();
        }
        for slot in slot_types {
            let path = match slot {
                Type::TYPE_STRING => match &string_repr {
                    StringRepr::Custom(path) => Some(path),
                    _ => return Err(invalid(proto_fqn, Reason::BufferRepresentation)),
                },
                Type::TYPE_BYTES => match &bytes_repr {
                    BytesRepr::Custom(path) => Some(path),
                    _ => return Err(invalid(proto_fqn, Reason::BufferRepresentation)),
                },
                _ => None,
            };
            if let Some(path) = path {
                if ctx.config.generate_json && !ctx.is_json_policy_type(path) {
                    return Err(invalid(proto_fqn, Reason::JsonPolicy));
                }
                let ty = crate::parse_custom_type_path(path)?;
                guards.push(quote! {__requires_wipe_owner::<#ty>();});
            }
        }
        // Imported messages must own their cleanup too. A Zeroize-only
        // implementation would not protect a child that fails before it can
        // be inserted into this parent.
        let message_slot = crate::message::find_map_entry(msg, field)
            .and_then(|entry| entry.field.iter().find(|slot| slot.number == Some(2)))
            .unwrap_or(field);
        let message_type = crate::impl_message::effective_type(ctx, message_slot, features);
        if matches!(message_type, Type::TYPE_MESSAGE | Type::TYPE_GROUP) {
            let ty = crate::message::scalar_or_message_type_nested(
                ctx,
                message_slot,
                scope.current_package,
                scope.nesting,
                features,
                resolver,
            )?;
            guards.push(quote! { __requires_wipe_owner::<#ty>(); });
        }
        if field.label.unwrap_or_default()
            == crate::generated::descriptor::field_descriptor_proto::Label::LABEL_REPEATED
        {
            let info = crate::message::classify_field(scope, msg, field, resolver)?;
            // This assertion is outside the message impl, where `Self` is
            // unavailable for recursive repeated fields.
            let ty = info.concrete_type();
            guards.push(quote! { __requires_wipe_owner::<#ty>(); });
        }
        if !crate::impl_message::is_real_oneof_member(field) {
            fields.push(ctx.field_ident(field_name, field.number.unwrap_or(0)));
        }
    }
    for (index, oneof) in msg.oneof_decl.iter().enumerate() {
        if msg.field.iter().any(|field| {
            crate::impl_message::is_real_oneof_member(field)
                && field.oneof_index.and_then(|i| usize::try_from(i).ok()) == Some(index)
        }) {
            fields.push(
                ctx.oneof_ident(
                    oneof
                        .name
                        .as_deref()
                        .ok_or(CodeGenError::MissingField("oneof.name"))?,
                ),
            );
        }
    }
    let guard_check = if guards.is_empty() {
        quote! {}
    } else {
        quote! {
            const _:fn()=||{
                fn __requires_wipe_owner<T: ::buffa::sensitive::Zeroize+::buffa::sensitive::ZeroizeOnDrop>(){}
                #(#guards)*
            };
        }
    };
    let allow = crate::message::deprecated_field_allow(ctx, msg, proto_fqn);
    Ok(quote! {
        #allow
        #guard_check
        #allow
        impl ::buffa::sensitive::Zeroize for #name {
            fn zeroize(&mut self) { #(::buffa::sensitive::Zeroize::zeroize(&mut self.#fields);)* }
        }
        impl ::buffa::sensitive::ZeroizeOnDrop for #name {}
        impl ::core::ops::Drop for #name {
            fn drop(&mut self) { ::buffa::sensitive::Zeroize::zeroize(self); }
        }
    })
}

pub(crate) fn oneof_impl(
    ctx: &CodeGenContext<'_>,
    proto_fqn: &str,
    name: &Ident,
    variants: impl Iterator<Item = (Ident, bool)>,
) -> TokenStream {
    if !ctx.zeroizes_message(proto_fqn) {
        return quote! {};
    }
    let arms: Vec<_> = variants.map(|(variant, boxed)| {
        if boxed { quote! { Self::#variant(value) => ::buffa::sensitive::Zeroize::zeroize(&mut **value), } }
        else { quote! { Self::#variant(value) => ::buffa::sensitive::Zeroize::zeroize(value), } }
    }).collect();
    quote! {
        impl ::buffa::sensitive::Zeroize for #name {
            fn zeroize(&mut self) { match self { #(#arms)* } }
        }
        impl ::buffa::sensitive::ZeroizeOnDrop for #name {}
        impl ::core::ops::Drop for #name { fn drop(&mut self) { ::buffa::sensitive::Zeroize::zeroize(self); } }
    }
}

/// Wipe view-owned scalar slots and collections without touching borrowed input.
pub(crate) fn view_impl(
    scope: MessageScope<'_>,
    msg: &DescriptorProto,
    name: &Ident,
    lazy: bool,
) -> Result<TokenStream, CodeGenError> {
    let ctx = scope.ctx;
    if !ctx.zeroizes_message(scope.proto_fqn) {
        return Ok(quote! {});
    }
    let mut fields = Vec::new();
    let mut child_checks = Vec::new();
    for field in &msg.field {
        let entry = crate::message::find_map_entry(msg, field);
        let child = entry
            .and_then(|entry| entry.field.iter().find(|slot| slot.number == Some(2)))
            .unwrap_or(field);
        if matches!(
            crate::impl_message::effective_type(ctx, child, scope.features),
            Type::TYPE_MESSAGE | Type::TYPE_GROUP
        ) {
            let deferred = lazy
                && entry.is_none()
                && !crate::impl_message::is_real_oneof_member(field)
                && crate::lazy_view::is_lazy_field(scope, field);
            let ty = if deferred {
                crate::view::resolve_lazy_view_ty_tokens(scope, child, &quote! {'a})?
            } else {
                crate::view::resolve_view_ty_tokens(scope, child, &quote! {'a})?
            };
            child_checks.push(quote! {__requires_view_wipe_owner::<#ty>();});
        }

        if crate::impl_message::is_real_oneof_member(field) {
            continue;
        }
        let repeated = field.label.unwrap_or_default()
            == crate::generated::descriptor::field_descriptor_proto::Label::LABEL_REPEATED;
        if !repeated
            && matches!(
                crate::impl_message::effective_type(ctx, field, scope.features),
                Type::TYPE_STRING | Type::TYPE_BYTES
            )
        {
            continue;
        }
        fields.push(
            ctx.field_ident(
                field
                    .name
                    .as_deref()
                    .ok_or(CodeGenError::MissingField("field.name"))?,
                field.number.unwrap_or(0),
            ),
        );
    }
    for (index, oneof) in msg.oneof_decl.iter().enumerate() {
        if msg.field.iter().any(|field| {
            crate::impl_message::is_real_oneof_member(field)
                && field.oneof_index.and_then(|i| usize::try_from(i).ok()) == Some(index)
        }) {
            fields.push(
                ctx.oneof_ident(
                    oneof
                        .name
                        .as_deref()
                        .ok_or(CodeGenError::MissingField("oneof.name"))?,
                ),
            );
        }
    }
    let checks = if child_checks.is_empty() {
        quote! {}
    } else {
        quote! {
            fn __requires_view_wipe_owner<T: ::buffa::sensitive::Zeroize+::buffa::sensitive::ZeroizeOnDrop>(){}
            #(#child_checks)*
        }
    };
    let allow = crate::message::deprecated_field_allow(ctx, msg, scope.proto_fqn);
    Ok(quote! {
        #allow
        impl<'a> ::buffa::sensitive::Zeroize for #name<'a> {
            fn zeroize(&mut self) { #checks #(::buffa::sensitive::Zeroize::zeroize(&mut self.#fields);)* }
        }
        impl<'a> ::buffa::sensitive::ZeroizeOnDrop for #name<'a> {}
        impl<'a> ::core::ops::Drop for #name<'a> {
            fn drop(&mut self) { ::buffa::sensitive::Zeroize::zeroize(self); }
        }
    })
}

pub(crate) fn view_oneof_impl(
    scope: MessageScope<'_>,
    fields: &[&crate::generated::descriptor::FieldDescriptorProto],
    name: &Ident,
    generics: &TokenStream,
) -> Result<TokenStream, CodeGenError> {
    if !scope.ctx.zeroizes_message(scope.proto_fqn) {
        return Ok(quote! {});
    }
    let arms = fields.iter().map(|field| {
        let name = field.name.as_deref().ok_or(CodeGenError::MissingField("field.name"))?;
        let variant = crate::oneof::oneof_variant_ident(name);
        Ok(match crate::impl_message::effective_type(scope.ctx, field, scope.features) {
            Type::TYPE_STRING | Type::TYPE_BYTES => quote! { Self::#variant(_) => {}, },
            Type::TYPE_MESSAGE | Type::TYPE_GROUP => quote! {
                Self::#variant(value) => ::buffa::sensitive::Zeroize::zeroize(&mut **value),
            },
            _ => quote! { Self::#variant(value) => ::buffa::sensitive::Zeroize::zeroize(value), },
        })
    }).collect::<Result<Vec<TokenStream>, CodeGenError>>()?;
    Ok(quote! {
        impl #generics ::buffa::sensitive::Zeroize for #name #generics {
            fn zeroize(&mut self) { match self { #(#arms)* } }
        }
        impl #generics ::buffa::sensitive::ZeroizeOnDrop for #name #generics {}
        impl #generics ::core::ops::Drop for #name #generics {
            fn drop(&mut self) { ::buffa::sensitive::Zeroize::zeroize(self); }
        }
    })
}

fn invalid(message: &str, reason: Reason) -> CodeGenError {
    CodeGenError::InvalidZeroizeConfiguration {
        message: message.into(),
        reason,
    }
}

/// Build a dropping owner before evaluating any fallible field initializer.
/// Otherwise Rust drops initialized fields on error, but plain scalar fields
/// in a partially initialized aggregate never reach the message's destructor.
pub(crate) fn owned_from_view(
    ctx: &CodeGenContext<'_>,
    proto_fqn: &str,
    owned: &TokenStream,
    fields: &[(Ident, TokenStream)],
) -> TokenStream {
    if !ctx.zeroizes_message(proto_fqn) {
        let initializers = fields.iter().map(|(name, value)| quote! { #name: #value, });
        return quote! {
            ::core::result::Result::Ok(#owned {
                #(#initializers)*
                ..::core::default::Default::default()
            })
        };
    }
    let assignments = fields
        .iter()
        .map(|(name, value)| quote! { __owned.#name = #value; });
    let mutable = if fields.is_empty() {
        quote! {}
    } else {
        quote! {mut}
    };
    quote! {
        let #mutable __owned = <#owned as ::core::default::Default>::default();
        #(#assignments)*
        ::core::result::Result::Ok(__owned)
    }
}
