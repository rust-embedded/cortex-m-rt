use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::spanned::Spanned;

use crate::input::{
    ExceptionArgs, ExceptionHandler, HandlerParamKind, InterruptArgs, ResourceParam, SimpleHandler,
};

/// Creates `static mut` items for every resource in `res`.
fn declare_resources<'a>(
    res: impl Iterator<Item = &'a ResourceParam> + 'a,
) -> impl Iterator<Item = TokenStream> + 'a {
    res.enumerate().map(|(i, res)| {
        let init = &res.init;
        let ty = &res.ty;
        let name = Ident::new(&format!("RESOURCE_{}", i), init.span());
        quote! {
            static mut #name: #ty = #init;
        }
    })
}

/// Generates a list of expressions that refer to the resources in `res`.
fn resource_arguments<'a>(
    res: impl Iterator<Item = &'a ResourceParam> + 'a,
) -> impl Iterator<Item = TokenStream> + 'a {
    res.enumerate().map(|(i, res)| {
        let res_ident = Ident::new(&format!("RESOURCE_{}", i), res.init.span());
        let cfgs = &res.cfgs;

        quote! {
            #(#cfgs)*
            &mut #res_ident
        }
    })
}

pub(crate) fn codegen_simple_handler(
    export_name: &str,
    must_diverge: bool,
    handler: &SimpleHandler,
) -> TokenStream {
    let resource_decls = declare_resources(handler.params.iter());

    let call = {
        let callee = &handler.func.sig.ident;
        let arguments = resource_arguments(handler.params.iter());

        quote! {
            #callee(
                #(#arguments),*
            )
        }
    };

    let ret_ty = match must_diverge {
        false => quote!(),
        true => quote!(-> !),
    };

    let handler_fn = &handler.func;

    quote! {
        const _: () = {
            #[export_name = #export_name]
            unsafe extern "C" fn cmrt_handler() #ret_ty {
                #(#resource_decls)*

                #call
            }
        };

        #handler_fn
    }
}

pub(crate) fn codegen_interrupt_handler(
    args: &InterruptArgs,
    handler: &SimpleHandler,
) -> TokenStream {
    let variant_path = args.path.clone();
    let mut interrupt_enum_type = args.path.clone();
    let variant = interrupt_enum_type.segments.pop().unwrap().into_value(); // remove variant

    let handler = codegen_simple_handler(&variant.ident.to_string(), false, handler);

    // FIXME: You can still define something like
    // ```
    // struct Trick;
    //
    // impl Trick {
    //     const RenamedVariant: Interrupt = Interrupt::Variant;
    // }
    // ```
    // and then use it via `#[interrupt(Trick::RenamedVariant)]`. We can fix this once cortex-m-rt
    // and cortex-m are merged, by requiring `Interrupt` to implement `cortex_m::InterruptNumber`,
    // which has a safety contract.

    quote! {
        const _: () = {
            // Assert that `interrupt_enum_type` is a type, and `variant_path` is an instance of it.
            let _: #interrupt_enum_type = #variant_path;
        };

        #handler
    }
}

pub(crate) fn codegen_exception_handler(
    args: &ExceptionArgs,
    must_diverge: bool,
    handler: &ExceptionHandler,
) -> TokenStream {
    let resource_decls =
        declare_resources(handler.params.iter().filter_map(|param| match &param.kind {
            HandlerParamKind::Resource(res) => Some(res),
            _ => None,
        }));

    let ret_ty = match must_diverge {
        false => quote!(),
        true => quote!(-> !),
    };

    let handler_fn = &handler.func;
    let export_name = args.name.to_string();

    quote! {
        const _: () = {
            #[export_name = #export_name]
            unsafe extern "C" fn cmrt_handler() #ret_ty {
                #(#resource_decls)*

                // TODO
                //#call
            }
        };

        #handler_fn
    }
}
