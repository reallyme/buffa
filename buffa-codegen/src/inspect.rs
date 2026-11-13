//! Checked borrowed inspection for cleanup-enabled message families.

use proc_macro2::TokenStream;
use quote::quote;

pub(crate) fn message_impl(
    generics: &TokenStream,
    ty: &TokenStream,
    pool: &TokenStream,
    get_arms: &[TokenStream],
    has_arms: &[TokenStream],
    allow: &TokenStream,
) -> TokenStream {
    let get_result = if get_arms.is_empty() {
        quote! { Err(InspectErrorReason::FieldNotMember) }
    } else {
        quote! {
            Ok(match field.number() {
                #(#get_arms)*
                _ => return Err(InspectErrorReason::FieldNotMember),
            })
        }
    };
    let has_result = if has_arms.is_empty() {
        quote! { false }
    } else {
        quote! { match field.number() { #(#has_arms)* _ => false, } }
    };
    quote! {
        #allow
        impl #generics ::buffa_descriptor::reflect::InspectMessage for #ty {
            fn message_descriptor(&self) -> &::buffa_descriptor::MessageDescriptor {
                #pool.message(Self::__buffa_reflect_message_index())
            }

            fn pool(&self) -> &::buffa::alloc::sync::Arc<::buffa_descriptor::DescriptorPool> {
                #pool
            }

            fn get(
                &self,
                field: &::buffa_descriptor::FieldDescriptor,
            ) -> ::core::result::Result<
                ::buffa_descriptor::reflect::InspectValueRef<'_>,
                ::buffa_descriptor::reflect::InspectErrorReason,
            > {
                use ::buffa_descriptor::reflect::{InspectErrorReason, InspectMessage};
                #[allow(unused_imports)]
                use ::buffa::Enumeration as _;
                let descriptor = InspectMessage::message_descriptor(self);
                if !descriptor.field(field.number()).is_some_and(|declared| {
                    ::core::ptr::eq(declared, field)
                }) {
                    return Err(InspectErrorReason::FieldNotMember);
                }
                #get_result
            }

            fn has(&self, field: &::buffa_descriptor::FieldDescriptor) -> bool {
                let descriptor = ::buffa_descriptor::reflect::InspectMessage::message_descriptor(self);
                if !descriptor.field(field.number()).is_some_and(|declared| {
                    ::core::ptr::eq(declared, field)
                }) {
                    return false;
                }
                #has_result
            }
        }

        impl #generics ::buffa_descriptor::reflect::InspectElement for #ty {
            fn as_inspect_value(&self) -> ::buffa_descriptor::reflect::InspectValueRef<'_> {
                ::buffa_descriptor::reflect::InspectValueRef::Message(
                    ::buffa_descriptor::reflect::InspectMessageRef::Borrowed(self),
                )
            }
        }

        impl #generics #ty {
            // Descriptor bytes and MessageName come from the same generation.
            // Share the existing memoized, schema-only reflection lookup.
            #[doc(hidden)]
            fn __buffa_reflect_message_index() -> ::buffa_descriptor::MessageIndex {
                static IDX: ::std::sync::OnceLock<::buffa_descriptor::MessageIndex> =
                    ::std::sync::OnceLock::new();
                *IDX.get_or_init(|| {
                    #pool
                        .message_index(<Self as ::buffa::MessageName>::FULL_NAME)
                        .expect("generated message is registered in the embedded descriptor pool")
                })
            }
        }
    }
}
