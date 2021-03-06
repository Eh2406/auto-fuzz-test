use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::fmt;
use syn::{
    Expr, Fields, FnArg, GenericArgument, Ident, ItemFn, ItemStruct, Member, Pat, PathArguments,
    Signature, Stmt, Type,
};

pub fn fuzz_struct(signature: &Signature, impl_type: Option<&Type>) -> Result<ItemStruct, Error> {
    // struct for function arguments template
    let mut fuzz_struct: ItemStruct = syn::parse2(quote! {
        #[derive(Arbitrary)]
        #[derive(Debug)]
        pub struct fuzz {
            a:u32,
            b:Box<u64>
        }
    })
    .unwrap();

    // Struct ident generation
    fuzz_struct.ident = match impl_type {
        Some(typ) => {
            if let Type::Path(path) = typ {
                format_ident!(
                    "__fuzz_struct_{}_{}",
                    &(path.path.segments.iter().next().unwrap().ident).to_string(),
                    &(*signature).ident.to_string()
                )
            } else {
                return Err(Error::ComplexSelfType);
            }
        }
        None => {
            format_ident!("__fuzz_struct_{}", &(*signature).ident.to_string())
        }
    };

    // Struct fields generation
    if let Fields::Named(ref mut fields) = fuzz_struct.fields {
        // Here comes an epic destructuring of syn types.
        // `else` parts of match arms on known data (`fuzz_struct` in this case) are
        // marked with unreachable! macro, as they are, ahem, unreachable.
        let default_boxed_variable = fields
            .named
            .pop()
            .expect(
                "Struct template must contain
                Boxed variable",
            )
            .into_value();
        let default_variable = fields
            .named
            .pop()
            .expect(
                "Struct template must contain
                unBoxed variable",
            )
            .into_value();
        for item in (*signature).inputs.iter() {
            match item {
                FnArg::Typed(i) => {
                    if let Pat::Ident(id) = &*i.pat {
                        match *i.ty.clone() {
                            Type::Reference(rf) => {
                                if let Type::Path(path) = *rf.elem.clone() {
                                    // `variable` is a new struct field
                                    let mut variable = default_boxed_variable.clone();
                                    variable.ident = Some(id.ident.clone());

                                    // Copying variable type
                                    if let Type::Path(ref mut new_path) = variable.ty {
                                        if let PathArguments::AngleBracketed(
                                            ref mut new_generic_arg,
                                        ) = new_path
                                            .path
                                            .segments
                                            .iter_mut()
                                            .next()
                                            .unwrap()
                                            .arguments
                                        {
                                            if let GenericArgument::Type(ref mut new_subpath) =
                                                new_generic_arg.args.iter_mut().next().unwrap()
                                            {
                                                *new_subpath = Type::Path(path);
                                            } else {
                                                unreachable!("Wrong boxed variable template");
                                            }
                                        } else {
                                            unreachable!("Wrong boxed variable template");
                                        }
                                    } else {
                                        unreachable!("Wrong boxed variable template");
                                    }
                                    // Pushing variable type for the struct field
                                    fields.named.push(variable);
                                } else {
                                    return Err(Error::ComplexArg);
                                }
                            }
                            Type::Path(path) => {
                                // `variable` is a new struct field
                                let mut variable = default_variable.clone();
                                variable.ident = Some(id.ident.clone());
                                // Copying variable type
                                variable.ty = Type::Path(path);
                                // Pushing variable type for the struct field
                                fields.named.push(variable);
                            }
                            _ => {
                                return Err(Error::ComplexArg);
                            }
                        };
                    } else {
                        return Err(Error::ComplexVariable);
                    }
                }
                FnArg::Receiver(res) => {
                    if let Some(ref impl_type) = impl_type {
                        if let Type::Path(_) = impl_type {
                            if res.reference.is_some() {
                                // `variable` is a new struct field
                                let mut variable = default_boxed_variable.clone();
                                variable.ident = Some(format_ident!("slf"));

                                // Copying variable type
                                if let Type::Path(ref mut new_path) = variable.ty {
                                    if let PathArguments::AngleBracketed(ref mut new_generic_arg) =
                                        new_path.path.segments.iter_mut().next().unwrap().arguments
                                    {
                                        if let GenericArgument::Type(ref mut new_subpath) =
                                            new_generic_arg.args.iter_mut().next().unwrap()
                                        {
                                            *new_subpath = (*impl_type).clone();
                                        } else {
                                            unreachable!("Wrong boxed variable template");
                                        }
                                    } else {
                                        unreachable!("Wrong boxed variable template");
                                    }
                                } else {
                                    unreachable!("Wrong boxed variable template");
                                }
                                // Pushing variable type for the struct field
                                fields.named.push(variable);
                            } else {
                                // `variable` is a new struct field
                                let mut variable = default_variable.clone();
                                variable.ident = Some(format_ident!("slf"));
                                // Copying variable type
                                variable.ty = (*impl_type).clone();
                                // Pushing variable type for the struct field
                                fields.named.push(variable);
                            }
                        } else {
                            return Err(Error::ComplexSelfType);
                        }
                    } else {
                        panic!("Self type must be supplied for method parsing")
                    }
                }
            }
        }
    } else {
        unreachable!("Struct template must contain named fields");
    }

    Ok(fuzz_struct)
}

pub fn fuzz_function(signature: &Signature, impl_type: Option<&Type>) -> Result<ItemFn, Error> {
    // Checking that the function meets our requirements
    if signature.asyncness.is_some() {
        return Err(Error::Async);
    }
    if signature.unsafety.is_some() {
        return Err(Error::Unsafe);
    }
    if signature.inputs.is_empty() {
        return Err(Error::Empty);
    }

    let mut fuzz_function: syn::ItemFn;

    if let Some(typ) = impl_type {
        match (*signature).inputs.first().unwrap() {
            FnArg::Receiver(_) => {
                // method harness template
                fuzz_function = syn::parse2(quote! {
                    pub fn fuzz(mut input:MyStruct) {
                        (input.slf).foo(input.a, &mut *input.b);
                    }
                })
                .unwrap();

                if let Stmt::Semi(Expr::MethodCall(method_call), _) =
                    &mut fuzz_function.block.stmts[0]
                {
                    // MethodCall inside fuzzing function
                    method_call.method = (*signature).ident.clone();

                    // Arguments for internal method call
                    let args = &mut method_call.args;
                    let default_borrowed_field = args.pop().unwrap().into_value();
                    let default_field = args.pop().unwrap().into_value();

                    for item in (*signature).inputs.iter().skip(1) {
                        match item {
                            FnArg::Typed(i) => {
                                if let Pat::Ident(id) = &*i.pat {
                                    match *i.ty.clone() {
                                        Type::Reference(rf) => {
                                            let mut new_field = default_borrowed_field.clone();
                                            if let Expr::Reference(ref mut new_rf) = new_field {
                                                // Copying borrow mutability
                                                new_rf.mutability = rf.mutability;
                                                // Copying field ident
                                                if let Expr::Unary(ref mut new_subfield) =
                                                    *new_rf.expr
                                                {
                                                    if let Expr::Field(ref mut new_unary_subfield) =
                                                        *new_subfield.expr
                                                    {
                                                        new_unary_subfield.member =
                                                            Member::Named(id.ident.clone());
                                                    } else {
                                                        unreachable!(
                                                            "Wrong borrowed field template"
                                                        );
                                                    }
                                                } else {
                                                    unreachable!("Wrong borrowed field template");
                                                }
                                            } else {
                                                unreachable!("Wrong borrowed field template");
                                            }

                                            // Pushing arguments to the function call
                                            args.push(new_field);
                                        }
                                        Type::Path(_) => {
                                            let mut new_field = default_field.clone();
                                            if let Expr::Field(ref mut f) = new_field {
                                                f.member = Member::Named(id.ident.clone());
                                            } else {
                                                unreachable!("Wrong unborrowed field template");
                                            }
                                            // Pushing arguments to the function call
                                            args.push(new_field);
                                        }
                                        _ => {
                                            return Err(Error::ComplexArg);
                                        }
                                    };
                                } else {
                                    return Err(Error::ComplexSelfType);
                                }
                            }
                            FnArg::Receiver(_) => {
                                return Err(Error::MultipleRes);
                            }
                        }
                    }
                } else {
                    unreachable!("Wrong method call template.")
                }
            }
            FnArg::Typed(_) => {
                // method harness template
                fuzz_function = syn::parse2(quote! {
                    pub fn fuzz(mut input:MyStruct) {
                        MyType::foo(input.a, &mut *input.b);
                    }
                })
                .unwrap();

                if let Stmt::Semi(Expr::Call(fn_call), _) = &mut fuzz_function.block.stmts[0] {
                    // FnCall inside fuzzing function
                    if let Expr::Path(path) = &mut *fn_call.func {
                        let mut segments_iter = path.path.segments.iter_mut();
                        if let Type::Path(type_path) = typ {
                            segments_iter.next().unwrap().ident =
                                type_path.path.segments.first().unwrap().ident.clone();
                        } else {
                            return Err(Error::ComplexMethodCall);
                        }
                        segments_iter.next().unwrap().ident = (*signature).ident.clone();
                    }

                    // Arguments for internal function call
                    let args = &mut fn_call.args;
                    let default_borrowed_field = args.pop().unwrap().into_value();
                    let default_field = args.pop().unwrap().into_value();

                    for item in (*signature).inputs.iter() {
                        match item {
                            FnArg::Typed(i) => {
                                if let Pat::Ident(id) = &*i.pat {
                                    match *i.ty.clone() {
                                        Type::Reference(rf) => {
                                            let mut new_field = default_borrowed_field.clone();
                                            if let Expr::Reference(ref mut new_rf) = new_field {
                                                // Copying borrow mutability
                                                new_rf.mutability = rf.mutability;
                                                // Copying field ident
                                                if let Expr::Unary(ref mut new_subfield) =
                                                    *new_rf.expr
                                                {
                                                    if let Expr::Field(ref mut new_unary_subfield) =
                                                        *new_subfield.expr
                                                    {
                                                        new_unary_subfield.member =
                                                            Member::Named(id.ident.clone());
                                                    } else {
                                                        unreachable!(
                                                            "Wrong borrowed field template"
                                                        );
                                                    }
                                                } else {
                                                    unreachable!("Wrong borrowed field template");
                                                }
                                            } else {
                                                unreachable!("Wrong borrowed field template");
                                            }

                                            // Pushing arguments to the function call
                                            args.push(new_field);
                                        }
                                        Type::Path(_) => {
                                            let mut new_field = default_field.clone();
                                            if let Expr::Field(ref mut f) = new_field {
                                                f.member = Member::Named(id.ident.clone());
                                            } else {
                                                unreachable!("Wrong unborrowed field template");
                                            }
                                            // Pushing arguments to the function call
                                            args.push(new_field);
                                        }
                                        _ => {
                                            return Err(Error::ComplexArg);
                                        }
                                    };
                                } else {
                                    return Err(Error::ComplexSelfType);
                                }
                            }
                            FnArg::Receiver(_) => {
                                panic!(
                                    "This macros can not be used for fuzzing methods, use #[create_cargofuzz_impl_harness]"
                                )
                            }
                        }
                    }
                } else {
                    unreachable!("Wrong generator call template.")
                }
            }
        }
    } else {
        // function harness template
        fuzz_function = syn::parse2(quote! {
            pub fn fuzz(mut input:MyStruct) {
                foo(input.a, &mut *input.b);
            }
        })
        .unwrap();

        if let Stmt::Semi(Expr::Call(fn_call), _) = &mut fuzz_function.block.stmts[0] {
            // FnCall inside fuzzing function
            if let Expr::Path(path) = &mut *fn_call.func {
                path.path.segments.iter_mut().next().unwrap().ident = (*signature).ident.clone();
            } else {
                unreachable!("Wrong function harness template.")
            }

            // Arguments for internal function call
            let args = &mut fn_call.args;
            let default_borrowed_field = args.pop().unwrap().into_value();
            let default_field = args.pop().unwrap().into_value();

            for item in (*signature).inputs.iter() {
                match item {
                    FnArg::Typed(i) => {
                        if let Pat::Ident(id) = &*i.pat {
                            match *i.ty.clone() {
                                Type::Reference(rf) => {
                                    let mut new_field = default_borrowed_field.clone();
                                    if let Expr::Reference(ref mut new_rf) = new_field {
                                        // Copying borrow mutability
                                        new_rf.mutability = rf.mutability;
                                        // Copying field ident
                                        if let Expr::Unary(ref mut new_subfield) = *new_rf.expr {
                                            if let Expr::Field(ref mut new_unary_subfield) =
                                                *new_subfield.expr
                                            {
                                                new_unary_subfield.member =
                                                    Member::Named(id.ident.clone());
                                            } else {
                                                unreachable!("Wrong borrowed field template");
                                            }
                                        } else {
                                            unreachable!("Wrong borrowed field template");
                                        }
                                    } else {
                                        unreachable!("Wrong borrowed field template");
                                    }

                                    // Pushing arguments to the function call
                                    args.push(new_field);
                                }
                                Type::Path(_) => {
                                    let mut new_field = default_field.clone();
                                    if let Expr::Field(ref mut f) = new_field {
                                        f.member = Member::Named(id.ident.clone());
                                    } else {
                                        unreachable!("Wrong unborrowed field template");
                                    }
                                    // Pushing arguments to the function call
                                    args.push(new_field);
                                }
                                _ => {
                                    return Err(Error::ComplexArg);
                                }
                            };
                        } else {
                            return Err(Error::ComplexVariable);
                        }
                    }
                    FnArg::Receiver(_) => {
                        panic!(
                            "This macros can not be used for fuzzing methods, use #[create_cargofuzz_impl_harness]"
                        );
                    }
                }
            }
        } else {
            unreachable!("Wrong function call template.");
        }
    }

    // Fuzing function input type
    if let FnArg::Typed(i) = fuzz_function.sig.inputs.iter_mut().next().unwrap() {
        if let Type::Path(typ) = &mut *i.ty {
            typ.path.segments.iter_mut().next().unwrap().ident = match impl_type {
                Some(typ) => {
                    if let Type::Path(path) = typ {
                        format_ident!(
                            "__fuzz_struct_{}_{}",
                            &(path.path.segments.iter().next().unwrap().ident).to_string(),
                            &(*signature).ident.to_string()
                        )
                    } else {
                        return Err(Error::ComplexSelfType);
                    }
                }
                None => {
                    format_ident!("__fuzz_struct_{}", &(*signature).ident.to_string())
                }
            };
        }
    }

    // Fuzzing function ident
    fuzz_function.sig.ident = match impl_type {
        Some(typ) => {
            if let Type::Path(path) = typ {
                format_ident!(
                    "__fuzz_{}_{}",
                    &(path.path.segments.iter().next().unwrap().ident).to_string(),
                    &(*signature).ident.to_string()
                )
            } else {
                return Err(Error::ComplexSelfType);
            }
        }
        None => {
            format_ident!("__fuzz_{}", &(*signature).ident.to_string())
        }
    };

    Ok(fuzz_function)
}

pub fn fuzz_harness(
    signature: &Signature,
    impl_type: Option<&Type>,
    crate_ident: &Ident,
    attr: &TokenStream,
) -> TokenStream {
    // Idents generation
    let arg_type = match impl_type {
        Some(typ) => {
            if let Type::Path(path) = typ {
                format_ident!(
                    "__fuzz_struct_{}_{}",
                    &(path.path.segments.iter().next().unwrap().ident).to_string(),
                    &(*signature).ident.to_string()
                )
            } else {
                unimplemented!("Complex self type.")
            }
        }
        None => {
            format_ident!("__fuzz_struct_{}", &(*signature).ident.to_string())
        }
    };

    let function_ident = match impl_type {
        Some(typ) => {
            if let Type::Path(path) = typ {
                format_ident!(
                    "__fuzz_{}_{}",
                    &(path.path.segments.iter().next().unwrap().ident).to_string(),
                    &(*signature).ident.to_string()
                )
            } else {
                unimplemented!("Complex self type.")
            }
        }
        None => {
            format_ident!("__fuzz_{}", &(*signature).ident.to_string())
        }
    };

    let path = {
        if attr.is_empty() {
            quote!(#crate_ident ::)
        } else {
            quote!(#crate_ident :: #attr ::)
        }
    };

    let code = quote!(
        // Autogenerated fuzzing harness.
        #![no_main]
        use libfuzzer_sys::fuzz_target;
        extern crate #crate_ident;

        fuzz_target!(|input: #path #arg_type| {
        #path #function_ident (input);
        });
    );

    code
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Unsafe,
    Async,
    Empty,
    ComplexArg,
    ComplexSelfType,
    MultipleRes,
    ComplexMethodCall,
    ComplexVariable,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let err_msg = match self {
            Error::Async => "Can not fuzz async functions.",
            Error::Unsafe => "unsafe functions can not be fuzzed automatically.",
            Error::Empty => "It is useless to fuzz function without input parameters.",
            Error::ComplexArg => "Type of the function must be either standalone, or borrowed standalone (like `&Type`, but not like `&(u32, String)`)",
            Error::ComplexSelfType => "Only implementations for simple (like `MyType`) types are supported",
            Error::MultipleRes => "Muptiple Self values in function args.",
            Error::ComplexMethodCall => "Complex method calls are not currently supported.",
            Error::ComplexVariable => "Complex variables (like `&mut *a`) are not supported",
        };

        write!(f, "{}", err_msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_tokens_eq::assert_tokens_eq;
    use pretty_assertions::assert_eq;
    use syn::ItemImpl;

    #[test]
    fn struct_no_borrows() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, crash_on_overflow: bool) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();

        let fuzz_struct_needed: ItemStruct = syn::parse2(quote! {
            #[derive(Arbitrary)]
            #[derive(Debug)]
            pub struct __fuzz_struct_maybe_checked_mul {
                a: u64,
                b: u64,
                crash_on_overflow: bool
            }
        })
        .unwrap();
        assert_eq!(fuzz_struct(&function.sig, None), Ok(fuzz_struct_needed));
    }

    #[test]
    fn struct_borrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul_borrowed(a: &mut u64, b: u64, crash_on_overflow: bool) {
                if crash_on_overflow {
                    *a = a.checked_mul(b).expect("Overflow has occurred");
                } else {
                    *a = a.overflowing_mul(b).0;
                }
            }
        })
        .unwrap();

        let fuzz_struct_needed: ItemStruct = syn::parse2(quote! {
            #[derive(Arbitrary)]
            #[derive(Debug)]
            pub struct __fuzz_struct_maybe_checked_mul_borrowed {
                a: Box<u64>,
                b: u64,
                crash_on_overflow: bool
            }
        })
        .unwrap();
        assert_eq!(fuzz_struct(&function.sig, None), Ok(fuzz_struct_needed));
    }

    #[test]
    fn struct_method_borrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn set_b(&mut self, b: u64) {
                self.b = b;
            }
        })
        .unwrap();

        let implementation: ItemImpl = syn::parse2(quote! {
            impl TestStruct {
            }
        })
        .unwrap();

        let fuzz_struct_needed: ItemStruct = syn::parse2(quote! {
            #[derive(Arbitrary)]
            #[derive(Debug)]
            pub struct __fuzz_struct_TestStruct_set_b {
                slf: Box<TestStruct>,
                b: u64
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_struct(&function.sig, Some(&implementation.self_ty)),
            Ok(fuzz_struct_needed)
        );
    }

    #[test]
    fn struct_method_unborrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn set_b(self, b: u64) -> u64 {
                self.b + b
            }
        })
        .unwrap();
        let implementation: ItemImpl = syn::parse2(quote! {
            impl TestStruct {
            }
        })
        .unwrap();

        let fuzz_struct_needed: ItemStruct = syn::parse2(quote! {
            #[derive(Arbitrary)]
            #[derive(Debug)]
            pub struct __fuzz_struct_TestStruct_set_b {
                slf: TestStruct,
                b: u64
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_struct(&function.sig, Some(&implementation.self_ty)),
            Ok(fuzz_struct_needed)
        );
    }

    #[test]
    fn struct_sliced_arg() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, crash_on_overflow: bool, sl: &[u32]) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();
        assert_eq!(fuzz_struct(&function.sig, None), Err(Error::ComplexArg));
    }

    #[test]
    fn struct_complex_variable() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, (c,d):(u32,u64)) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_struct(&function.sig, None),
            Err(Error::ComplexVariable)
        );
    }

    #[test]
    fn function_unborrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, crash_on_overflow: bool) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();

        let fuzz_function_needed: ItemFn = syn::parse2(quote! {
            pub fn __fuzz_maybe_checked_mul(mut input:__fuzz_struct_maybe_checked_mul) {
                maybe_checked_mul(input.a, input.b, input.crash_on_overflow);
            }
        })
        .unwrap();
        assert_eq!(fuzz_function(&function.sig, None), Ok(fuzz_function_needed));
    }

    #[test]
    fn function_borrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul_borrowed(a: &mut u64, b: u64, crash_on_overflow: bool) {
                if crash_on_overflow {
                    *a = a.checked_mul(b).expect("Overflow has occurred");
                } else {
                    *a = a.overflowing_mul(b).0;
                }
            }
        })
        .unwrap();

        let fuzz_function_needed: ItemFn = syn::parse2(
            quote! {
                pub fn __fuzz_maybe_checked_mul_borrowed(mut input:__fuzz_struct_maybe_checked_mul_borrowed) {
                    maybe_checked_mul_borrowed(&mut *input.a, input.b, input.crash_on_overflow);
                }
            }
        ).unwrap();
        assert_eq!(fuzz_function(&function.sig, None), Ok(fuzz_function_needed));
    }

    #[test]
    fn function_sliced_arg() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, crash_on_overflow: bool, sl: &[u32]) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();

        let fuzz_function_needed: ItemFn = syn::parse2(quote! {
            pub fn __fuzz_maybe_checked_mul(mut input:__fuzz_struct_maybe_checked_mul) {
                maybe_checked_mul(input.a, input.b, input.crash_on_overflow, & *input.sl);
            }
        })
        .unwrap();
        assert_eq!(fuzz_function(&function.sig, None), Ok(fuzz_function_needed));
    }

    #[test]
    fn function_complex_variable() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, (c,d):(u32,u64)) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_function(&function.sig, None),
            Err(Error::ComplexVariable)
        );
    }

    #[test]
    fn function_empty() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul_borrowed() {
                if crash_on_overflow {
                    *a = a.checked_mul(b).expect("Overflow has occurred");
                } else {
                    *a = a.overflowing_mul(b).0;
                }
            }
        })
        .unwrap();
        assert_eq!(fuzz_function(&function.sig, None), Err(Error::Empty));
    }

    #[test]
    fn method_unborrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn set_b(self, b: u64) -> u64 {
                self.b + b
            }
        })
        .unwrap();
        let implementation: ItemImpl = syn::parse2(quote! {
            impl TestStruct {
            }
        })
        .unwrap();
        let fuzz_function_needed: ItemFn = syn::parse2(quote! {
            pub fn __fuzz_TestStruct_set_b(mut input: __fuzz_struct_TestStruct_set_b) {
                    (input.slf).set_b(input.b);
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_function(&function.sig, Some(&implementation.self_ty)),
            Ok(fuzz_function_needed)
        );
    }

    #[test]
    fn method_borrowed() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn set_b(&mut self, b: u64) {
                self.b = b;
            }
        })
        .unwrap();
        let implementation: ItemImpl = syn::parse2(quote! {
            impl TestStruct {
            }
        })
        .unwrap();
        let fuzz_function_needed: ItemFn = syn::parse2(quote! {
            pub fn __fuzz_TestStruct_set_b(mut input: __fuzz_struct_TestStruct_set_b) {
                    (input.slf).set_b(input.b);
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_function(&function.sig, Some(&implementation.self_ty)),
            Ok(fuzz_function_needed)
        );
    }

    #[test]
    fn method_generator() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn new(a:u64, b:u64) -> TestStruct {
                TestStruct {a,b}
            }
        })
        .unwrap();
        let implementation: ItemImpl = syn::parse2(quote! {
            impl TestStruct {
            }
        })
        .unwrap();
        let fuzz_function_needed: ItemFn = syn::parse2(quote! {
            pub fn __fuzz_TestStruct_new(mut input: __fuzz_struct_TestStruct_new) {
                TestStruct::new(input.a, input.b);
            }
        })
        .unwrap();
        assert_eq!(
            fuzz_function(&function.sig, Some(&implementation.self_ty)),
            Ok(fuzz_function_needed)
        );
    }

    #[test]
    fn harness() {
        let function: ItemFn = syn::parse2(quote! {
            pub fn maybe_checked_mul(a: u64, b: u64, crash_on_overflow: bool) -> u64 {
                if crash_on_overflow {
                    a.checked_mul(b).expect("Overflow has occurred")
                } else {
                    a.overflowing_mul(b).0
                }
            }
        })
        .unwrap();

        let fuzz_harness_needed = quote! {
            #![no_main]
            use libfuzzer_sys::fuzz_target;
            extern crate lib;

            fuzz_target!( |input: lib::foo::bar::__fuzz_struct_maybe_checked_mul| {
                    lib::foo::bar::__fuzz_maybe_checked_mul(input);
                }
            );
        };

        let attrs = quote!(foo::bar);
        let crate_ident = format_ident!("lib");
        assert_tokens_eq!(
            fuzz_harness(&function.sig, None, &crate_ident, &attrs),
            fuzz_harness_needed
        );
    }
}
