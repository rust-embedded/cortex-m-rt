//! Internal implementation details of `cortex-m-rt`.
//!
//! Do not use this crate directly.

mod codegen;
mod input;

extern crate proc_macro;

use input::{ExceptionArgs, ExceptionHandler, InterruptArgs, PreInitArgs, SimpleHandler};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::collections::HashSet;
use std::iter;
use syn::{
    parse, parse_macro_input, spanned::Spanned, AttrStyle, Attribute, FnArg, Ident, Item, ItemFn,
    ItemStatic, ReturnType, Stmt, Type, Visibility,
};

#[proc_macro_attribute]
pub fn entry(args: TokenStream, input: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return parse::Error::new(Span::call_site(), "this attribute accepts no arguments")
            .to_compile_error()
            .into();
    }

    let handler = parse_macro_input!(input as SimpleHandler);
    codegen::codegen_simple_handler("main", true, &handler).into()
}

#[derive(Debug, PartialEq)]
enum Exception {
    DefaultHandler,
    HardFault,
    NonMaskableInt,
    Other,
}

#[proc_macro_attribute]
pub fn exception(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as ExceptionArgs);
    let handler = parse_macro_input!(input as ExceptionHandler);

    if let Err(error) = check_attr_whitelist(&f.attrs, WhiteListCaller::Exception) {
        return error;
    }

    let fspan = f.span();
    let ident = f.sig.ident.clone();

    let ident_s = ident.to_string();
    let exn = match &*ident_s {
        "DefaultHandler" => Exception::DefaultHandler,
        "HardFault" => Exception::HardFault,
        "NonMaskableInt" => Exception::NonMaskableInt,
        "MemoryManagement" | "BusFault" | "UsageFault" | "SecureFault" | "SVCall"
        | "DebugMonitor" | "PendSV" | "SysTick" => Exception::Other,
        _ => {
            return parse::Error::new(ident.span(), "This is not a valid exception name")
                .to_compile_error()
                .into();
        }
    };

    if f.sig.unsafety.is_none() {
        match exn {
            Exception::DefaultHandler | Exception::HardFault | Exception::NonMaskableInt => {
                // These are unsafe to define.
                let name = if exn == Exception::DefaultHandler {
                    format!("`DefaultHandler`")
                } else {
                    format!("`{:?}` handler", exn)
                };
                return parse::Error::new(ident.span(), format_args!("defining a {} is unsafe and requires an `unsafe fn` (see the cortex-m-rt docs)", name))
                    .to_compile_error()
                    .into();
            }
            Exception::Other => {}
        }
    }

    // Emit a reference to the `Exception` variant corresponding to our exception.
    // This will fail compilation when the target doesn't have that exception.
    let assertion = match exn {
        Exception::Other => {
            quote! {
                const _: () = {
                    let _ = cortex_m_rt::Exception::#ident;
                };
            }
        }
        _ => quote!(),
    };

    let handler = match exn {
        Exception::DefaultHandler => {
            let valid_signature = f.sig.constness.is_none()
                && f.vis == Visibility::Inherited
                && f.sig.abi.is_none()
                && f.sig.inputs.len() == 1
                && f.sig.generics.params.is_empty()
                && f.sig.generics.where_clause.is_none()
                && f.sig.variadic.is_none()
                && match f.sig.output {
                    ReturnType::Default => true,
                    ReturnType::Type(_, ref ty) => match **ty {
                        Type::Tuple(ref tuple) => tuple.elems.is_empty(),
                        Type::Never(..) => true,
                        _ => false,
                    },
                };

            if !valid_signature {
                return parse::Error::new(
                    fspan,
                    "`DefaultHandler` must have signature `unsafe fn(i16) [-> !]`",
                )
                .to_compile_error()
                .into();
            }

            f.sig.ident = Ident::new(&format!("__cortex_m_rt_{}", f.sig.ident), Span::call_site());
            let tramp_ident = Ident::new(&format!("{}_trampoline", f.sig.ident), Span::call_site());
            let ident = &f.sig.ident;

            let (ref cfgs, ref attrs) = extract_cfgs(f.attrs.clone());

            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[doc(hidden)]
                #[export_name = #ident_s]
                pub unsafe extern "C" fn #tramp_ident() {
                    extern crate core;

                    const SCB_ICSR: *const u32 = 0xE000_ED04 as *const u32;

                    let irqn = unsafe { (core::ptr::read_volatile(SCB_ICSR) & 0x1FF) as i16 - 16 };

                    #ident(irqn)
                }

                #f
            )
        }
        Exception::HardFault => {
            let valid_signature = f.sig.constness.is_none()
                && f.vis == Visibility::Inherited
                && f.sig.abi.is_none()
                && f.sig.inputs.len() == 1
                && match &f.sig.inputs[0] {
                    FnArg::Typed(arg) => match arg.ty.as_ref() {
                        Type::Reference(r) => r.lifetime.is_none() && r.mutability.is_none(),
                        _ => false,
                    },
                    _ => false,
                }
                && f.sig.generics.params.is_empty()
                && f.sig.generics.where_clause.is_none()
                && f.sig.variadic.is_none()
                && match f.sig.output {
                    ReturnType::Default => false,
                    ReturnType::Type(_, ref ty) => match **ty {
                        Type::Never(_) => true,
                        _ => false,
                    },
                };

            if !valid_signature {
                return parse::Error::new(
                    fspan,
                    "`HardFault` handler must have signature `unsafe fn(&ExceptionFrame) -> !`",
                )
                .to_compile_error()
                .into();
            }

            f.sig.ident = Ident::new(&format!("__cortex_m_rt_{}", f.sig.ident), Span::call_site());
            let tramp_ident = Ident::new(&format!("{}_trampoline", f.sig.ident), Span::call_site());
            let ident = &f.sig.ident;

            let (ref cfgs, ref attrs) = extract_cfgs(f.attrs.clone());

            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[doc(hidden)]
                #[export_name = "HardFault"]
                // Only emit link_section when building for embedded targets,
                // because some hosted platforms (used to check the build)
                // cannot handle the long link section names.
                #[cfg_attr(target_os = "none", link_section = ".HardFault.user")]
                pub unsafe extern "C" fn #tramp_ident(frame: &::cortex_m_rt::ExceptionFrame) {
                    #ident(frame)
                }

                #f
            )
        }
        Exception::NonMaskableInt | Exception::Other => {
            let valid_signature = f.sig.constness.is_none()
                && f.vis == Visibility::Inherited
                && f.sig.abi.is_none()
                && f.sig.inputs.is_empty()
                && f.sig.generics.params.is_empty()
                && f.sig.generics.where_clause.is_none()
                && f.sig.variadic.is_none()
                && match f.sig.output {
                    ReturnType::Default => true,
                    ReturnType::Type(_, ref ty) => match **ty {
                        Type::Tuple(ref tuple) => tuple.elems.is_empty(),
                        Type::Never(..) => true,
                        _ => false,
                    },
                };

            if !valid_signature {
                return parse::Error::new(
                    fspan,
                    "`#[exception]` handlers other than `DefaultHandler` and `HardFault` must have \
                     signature `[unsafe] fn() [-> !]`",
                )
                .to_compile_error()
                .into();
            }

            let (statics, stmts) = match extract_static_muts(f.block.stmts) {
                Err(e) => return e.to_compile_error().into(),
                Ok(x) => x,
            };

            f.sig.ident = Ident::new(&format!("__cortex_m_rt_{}", f.sig.ident), Span::call_site());
            f.sig.inputs.extend(statics.iter().map(|statik| {
                let ident = &statik.ident;
                let ty = &statik.ty;
                let attrs = &statik.attrs;
                syn::parse::<FnArg>(
                    quote!(#[allow(non_snake_case)] #(#attrs)* #ident: &mut #ty).into(),
                )
                .unwrap()
            }));
            f.block.stmts = iter::once(
                syn::parse2(quote! {{
                    // check that this exception actually exists
                    exception::#ident;
                }})
                .unwrap(),
            )
            .chain(stmts)
            .collect();

            let tramp_ident = Ident::new(&format!("{}_trampoline", f.sig.ident), Span::call_site());
            let ident = &f.sig.ident;

            let resource_args = statics
                .iter()
                .map(|statik| {
                    let (ref cfgs, ref attrs) = extract_cfgs(statik.attrs.clone());
                    let ident = &statik.ident;
                    let ty = &statik.ty;
                    let expr = &statik.expr;
                    quote! {
                        #(#cfgs)*
                        {
                            #(#attrs)*
                            static mut #ident: #ty = #expr;
                            &mut #ident
                        }
                    }
                })
                .collect::<Vec<_>>();

            let (ref cfgs, ref attrs) = extract_cfgs(f.attrs.clone());

            quote!(
                #(#cfgs)*
                #(#attrs)*
                #[doc(hidden)]
                #[export_name = #ident_s]
                pub unsafe extern "C" fn #tramp_ident() {
                    #ident(
                        #(#resource_args),*
                    )
                }

                #f
            )
        }
    };

    quote!(
        #assertion
        #handler
    )
    .into()
}

#[proc_macro_attribute]
pub fn interrupt(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as InterruptArgs);
    let handler = parse_macro_input!(input as SimpleHandler);

    if let Err(e) = handler.reject_static_resources() {
        return e.to_compile_error().into();
    }

    codegen::codegen_interrupt_handler(&args, &handler).into()
}

#[proc_macro_attribute]
pub fn pre_init(args: TokenStream, input: TokenStream) -> TokenStream {
    let _args = parse_macro_input!(args as PreInitArgs);
    let f = parse_macro_input!(input as ItemFn);

    // check the function signature
    let valid_signature = f.sig.constness.is_none()
        && f.vis == Visibility::Inherited
        && f.sig.abi.is_none()
        && f.sig.inputs.is_empty()
        && f.sig.generics.params.is_empty()
        && f.sig.generics.where_clause.is_none()
        && f.sig.variadic.is_none()
        && match f.sig.output {
            ReturnType::Default => true,
            ReturnType::Type(_, ref ty) => match **ty {
                Type::Tuple(ref tuple) => tuple.elems.is_empty(),
                _ => false,
            },
        };

    if !valid_signature {
        return parse::Error::new(
            f.span(),
            "`#[pre_init]` function must have signature `fn()`",
        )
        .to_compile_error()
        .into();
    }

    if let Err(error) = check_attr_whitelist(&f.attrs, WhiteListCaller::PreInit) {
        return error;
    }

    // XXX should we blacklist other attributes?
    let attrs = f.attrs;
    let ident = f.sig.ident;
    let block = f.block;

    quote!(
        #[export_name = "__pre_init"]
        #(#attrs)*
        extern "C" fn #ident() #block
    )
    .into()
}

/// Extracts `static mut` vars from the beginning of the given statements
fn extract_static_muts(
    stmts: impl IntoIterator<Item = Stmt>,
) -> Result<(Vec<ItemStatic>, Vec<Stmt>), parse::Error> {
    let mut istmts = stmts.into_iter();

    let mut seen = HashSet::new();
    let mut statics = vec![];
    let mut stmts = vec![];
    while let Some(stmt) = istmts.next() {
        match stmt {
            Stmt::Item(Item::Static(var)) => {
                if var.mutability.is_some() {
                    if seen.contains(&var.ident) {
                        return Err(parse::Error::new(
                            var.ident.span(),
                            format!("the name `{}` is defined multiple times", var.ident),
                        ));
                    }

                    seen.insert(var.ident.clone());
                    statics.push(var);
                } else {
                    stmts.push(Stmt::Item(Item::Static(var)));
                }
            }
            _ => {
                stmts.push(stmt);
                break;
            }
        }
    }

    stmts.extend(istmts);

    Ok((statics, stmts))
}

fn extract_cfgs(attrs: Vec<Attribute>) -> (Vec<Attribute>, Vec<Attribute>) {
    let mut cfgs = vec![];
    let mut not_cfgs = vec![];

    for attr in attrs {
        if eq(&attr, "cfg") {
            cfgs.push(attr);
        } else {
            not_cfgs.push(attr);
        }
    }

    (cfgs, not_cfgs)
}

enum WhiteListCaller {
    Entry,
    Exception,
    Interrupt,
    PreInit,
}

fn check_attr_whitelist(attrs: &[Attribute], caller: WhiteListCaller) -> Result<(), TokenStream> {
    let whitelist = &[
        "doc",
        "link_section",
        "cfg",
        "allow",
        "warn",
        "deny",
        "forbid",
        "cold",
    ];

    'o: for attr in attrs {
        for val in whitelist {
            if eq(&attr, &val) {
                continue 'o;
            }
        }

        let err_str = match caller {
            WhiteListCaller::Entry => "this attribute is not allowed on a cortex-m-rt entry point",
            WhiteListCaller::Exception => {
                "this attribute is not allowed on an exception handler controlled by cortex-m-rt"
            }
            WhiteListCaller::Interrupt => {
                "this attribute is not allowed on an interrupt handler controlled by cortex-m-rt"
            }
            WhiteListCaller::PreInit => {
                "this attribute is not allowed on a pre-init controlled by cortex-m-rt"
            }
        };

        return Err(parse::Error::new(attr.span(), &err_str)
            .to_compile_error()
            .into());
    }

    Ok(())
}

/// Returns `true` if `attr.path` matches `name`
fn eq(attr: &Attribute, name: &str) -> bool {
    attr.style == AttrStyle::Outer && attr.path.is_ident(name)
}
