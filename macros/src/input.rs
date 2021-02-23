use syn::{
    parse::{Error, Parse, ParseStream},
    Attribute, Expr, Ident, ItemFn, Path, Stmt, Token, Type,
};

/// Overridable exceptions. This mirrors `Exception` in the main crate, but without the `#[cfg]`s
/// and with additional "targets" like `DefaultHandler`.
#[derive(Clone, Copy)]
pub(crate) enum ExceptionHandlerTarget {
    DefaultHandler,
    NonMaskableInt,
    HardFault,
    MemoryManagement,
    BusFault,
    UsageFault,
    SecureFault,
    SVCall,
    DebugMonitor,
    PendSV,
    SysTick,
}

impl ExceptionHandlerTarget {
    fn parse(name: &Ident) -> syn::Result<Self> {
        Ok(match &*name.to_string() {
            "DefaultHandler" => Self::DefaultHandler,
            "NonMaskableInt" => Self::NonMaskableInt,
            "HardFault" => Self::HardFault,
            "MemoryManagement" => Self::MemoryManagement,
            "BusFault" => Self::BusFault,
            "UsageFault" => Self::UsageFault,
            "SecureFault" => Self::SecureFault,
            "SVCall" => Self::SVCall,
            "DebugMonitor" => Self::DebugMonitor,
            "PendSV" => Self::PendSV,
            "SysTick" => Self::SysTick,
            inv => {
                return Err(Error::new_spanned(
                    name,
                    format!("invalid exception name `{}`", inv),
                ))
            }
        })
    }

    /// Some exceptions are unsafe to handle since they are unmaskable, which breaks critical
    /// sections.
    pub(crate) fn is_unsafe_to_define(self) -> bool {
        match self {
            Self::NonMaskableInt
            | Self::HardFault
            // `DefaultHandler` cannot handle `HardFault`, but it does handle NMIs and is thus
            // unsafe.
            | Self::DefaultHandler => true,
            Self::MemoryManagement
            | Self::BusFault
            | Self::UsageFault
            | Self::SecureFault
            | Self::SVCall
            | Self::DebugMonitor
            | Self::PendSV
            | Self::SysTick => false,
        }
    }
}

/// `#[interrupt(path::to::Interrupt::Variant)]`.
///
/// The path is required to have at least 2 components. This is to ensure the variant matches the
/// name of the symbol (otherwise users could `use Enum::Variant as Other;`).
pub(crate) struct InterruptArgs {
    pub(crate) path: Path,
}

impl Parse for InterruptArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let this = Self {
            path: input.parse()?,
        };

        // The path must be "plain" (no type parameters) and have at least 2 segments.
        if this.path.segments.len() < 2 {
            return Err(Error::new_spanned(
                this.path,
                "path must be of the form `Enum::Variant` (just `Variant` is not allowed)",
            ));
        }

        if !this
            .path
            .segments
            .iter()
            .all(|segment| matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(Error::new_spanned(
                this.path,
                "path must not contain type, lifetime, or const parameters",
            ));
        }

        Ok(this)
    }
}

/// `#[exception(<unsafe?> Name)]`
pub(crate) struct ExceptionArgs {
    pub(crate) unsafe_token: Option<Token![unsafe]>,
    pub(crate) name: Ident,
    pub(crate) exception: ExceptionHandlerTarget,
}

impl Parse for ExceptionArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        let this = Self {
            unsafe_token: input.parse()?,
            exception: ExceptionHandlerTarget::parse(&name)?,
            name: name.clone(),
        };

        if this.exception.is_unsafe_to_define() && this.unsafe_token.is_none() {
            return Err(Error::new_spanned(
                &name,
                format!("it is unsafe to handle `{}`", name),
            ));
        }

        Ok(this)
    }
}

/// `#[pre_init(unsafe)]`
pub struct PreInitArgs {
    unsafe_token: Token![unsafe],
}

impl Parse for PreInitArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(Self {
            unsafe_token: input.parse()?,
        })
    }
}

pub(crate) struct ResourceParam {
    /// The initializing expression. Must be valid as the initializer of a `static`.
    pub(crate) init: Expr,

    /// The type of the resource. The handler must accept a mutable reference to this type as an
    /// argument.
    pub(crate) ty: Type,

    /// `#[cfg]` attributes that were applied to the parameter.
    pub(crate) cfgs: Vec<Attribute>,

    /// `true` if the parameter takes a `&'static mut`, `false` if it takes a non-static
    /// reference.
    is_static: bool,
}

impl ResourceParam {
    fn parse(attr: &Attribute, cfgs: &[Attribute], ty: &Type) -> syn::Result<Self> {
        let init = attr.parse_args()?;
        match ty {
            Type::Reference(r) if r.mutability.is_some() => {
                let ty = (*r.elem).clone();

                let is_static = match &r.lifetime {
                    Some(lt) if lt.ident.to_string() == "static" => true,
                    None => false,
                    Some(lt) => {
                        return Err(Error::new_spanned(
                            lt,
                            "explicit lifetime annotations besides `'static` \
                            are not allowed on resource parameters",
                        ));
                    }
                };

                Ok(ResourceParam {
                    init,
                    ty,
                    cfgs: cfgs.to_vec(),
                    is_static,
                })
            }
            _ => {
                return Err(Error::new_spanned(
                    ty,
                    "resource parameters must have type `&mut T`",
                ));
            }
        }
    }

    fn reject_static_resource(&self) -> syn::Result<()> {
        if self.is_static {
            Err(Error::new_spanned(
                &self.ty,
                "this resource cannot use the `'static` lifetime",
            ))
        } else {
            Ok(())
        }
    }
}

pub(crate) struct HandlerParam {
    pub attr: Attribute,
    pub kind: HandlerParamKind,
}

pub(crate) enum HandlerParamKind {
    /// A resource, defined with the `#[init(<expr>)]` attribute.
    Resource(ResourceParam),

    /// `#[irqn]` denotes that the IRQ number should be passed as an argument of type `i16`.
    ///
    /// Only valid on a `DefaultHandler`.
    Irqn,

    /// `#[exception_frame]` annotates an attribute of type `&mut cortex_m_rt::ExceptionFrame`,
    /// which is passed a mutable reference to the auto-stacked data on exception entry.
    ///
    /// Only valid on a `HardFault` exception handler.
    ExceptionFrame,
}

impl HandlerParamKind {
    fn parse(attr: &Attribute, cfgs: &[Attribute], ty: &Type) -> Option<syn::Result<Self>> {
        let ident = attr.path.get_ident()?;
        match &*ident.to_string() {
            "init" => Some(ResourceParam::parse(attr, cfgs, ty).map(Self::Resource)),
            "irqn" => {
                if !attr.tokens.is_empty() {
                    return Some(Err(Error::new_spanned(
                        &attr.tokens,
                        "`#[irqn]` does not take arguments",
                    )));
                }
                Some(Ok(Self::Irqn))
            }
            "exception_frame" => {
                if !attr.tokens.is_empty() {
                    return Some(Err(Error::new_spanned(
                        &attr.tokens,
                        "`#[exception_frame]` does not take arguments",
                    )));
                }
                Some(Ok(Self::ExceptionFrame))
            }
            _ => None,
        }
    }
}

/// An exception handler function, which may declare exception number and frame arguments, as well
/// as resources.
pub(crate) struct ExceptionHandler {
    pub(crate) func: ItemFn,
    pub(crate) params: Vec<HandlerParam>,
}

impl Parse for ExceptionHandler {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut func = parse_handler_base(input)?;
        let params = extract_handler_params(&mut func)?;

        // NOTE: We can not reliably check the return type here. Instead, the macro emits code to
        // unify the return value with `()` or `!`, depending on the handler.

        Ok(Self { func, params })
    }
}

impl ExceptionHandler {
    fn reject_static_resources(&self) -> syn::Result<()> {
        for param in &self.params {
            if let HandlerParamKind::Resource(res) = &param.kind {
                res.reject_static_resource()?;
            }
        }

        Ok(())
    }
}

/// A "simple" handler function that may only define resource parameters.
pub(crate) struct SimpleHandler {
    pub(crate) func: ItemFn,
    pub(crate) params: Vec<ResourceParam>,
}

impl Parse for SimpleHandler {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut func = parse_handler_base(input)?;
        let params = extract_handler_params(&mut func)?;

        let params = params
            .into_iter()
            .map(|param| match param.kind {
                HandlerParamKind::Resource(it) => Ok(it),
                HandlerParamKind::Irqn => Err(Error::new_spanned(
                    param.attr,
                    "`#[irqn]` is not allowed on this handler",
                )),
                HandlerParamKind::ExceptionFrame => Err(Error::new_spanned(
                    param.attr,
                    "`#[exception_frame]` is not allowed on this handler",
                )),
            })
            .collect::<syn::Result<Vec<_>>>()?;

        Ok(Self { func, params })
    }
}

impl SimpleHandler {
    pub(crate) fn reject_static_resources(&self) -> syn::Result<()> {
        for param in &self.params {
            param.reject_static_resource()?;
        }

        Ok(())
    }
}

fn parse_handler_base(input: ParseStream) -> syn::Result<ItemFn> {
    let f: ItemFn = input.parse()?;
    if let Some(asyncness) = &f.sig.asyncness {
        return Err(Error::new_spanned(
            asyncness,
            "interrupt and exception handlers must not be `async`",
        ));
    }

    if let Some(variadic) = &f.sig.variadic {
        return Err(Error::new_spanned(
            variadic.dots,
            "interrupt and exception handlers must not be variadic",
        ));
    }

    if !f.sig.generics.params.is_empty() {
        return Err(Error::new_spanned(
            f.sig.generics.params,
            "interrupt and exception handlers must not be generic",
        ));
    }

    if f.sig.generics.where_clause.is_some() {
        return Err(Error::new_spanned(
            f.sig.generics.where_clause,
            "interrupt and exception handlers must not have where-clauses",
        ));
    }

    Ok(f)
}

fn extract_handler_params(func: &mut ItemFn) -> syn::Result<Vec<HandlerParam>> {
    let mut params = Vec::new();
    for param in &mut func.sig.inputs {
        match param {
            syn::FnArg::Receiver(_) => {
                return Err(Error::new_spanned(
                    param,
                    "interrupt and exception handlers must not have a `self` parameter",
                ));
            }
            syn::FnArg::Typed(pat_type) => {
                let mut handler_params = Vec::new();
                let ty = &pat_type.ty;
                let cfgs = extract_cfgs(&pat_type.attrs);
                pat_type
                    .attrs
                    .retain(|attr| match HandlerParamKind::parse(attr, &cfgs, ty) {
                        Some(it) => {
                            handler_params.push((attr.clone(), it));
                            false
                        }
                        None => true,
                    });

                if handler_params.is_empty() {
                    return Err(Error::new_spanned(
                        pat_type,
                        "handler parameters must have an attribute denoting their type \
                        (try `#[init(<initial value>)]`)",
                    ));
                }

                if handler_params.len() > 1 {
                    return Err(Error::new_spanned(
                        pat_type,
                        "only one attribute allowed on this parameter",
                    ));
                }

                let (attr, param) = handler_params.pop().unwrap();
                params.push(HandlerParam { attr, kind: param? });
            }
        }
    }

    Ok(params)
}

fn extract_cfgs(attrs: &[Attribute]) -> Vec<Attribute> {
    attrs
        .iter()
        .filter(|attr| attr.path.is_ident("cfg"))
        .cloned()
        .collect()
}

fn is_unsafe_impl(func: &ItemFn) -> bool {
    match &*func.block.stmts {
        [Stmt::Expr(Expr::Unsafe(_))] | [Stmt::Semi(Expr::Unsafe(_), _)] => true,
        _ => false,
    }
}
