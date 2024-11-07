use proc_macro2::{Punct, Spacing};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{self, Attribute, DataStruct, Fields, NestedMeta};

#[proc_macro_derive(FromRecordStream, attributes(field_args, vector_size, from_record))]
pub fn from_record_dervive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: syn::DeriveInput = syn::parse(input).unwrap();
    impl_macro(&ast)
}

// struct VectorSize {
//     min: Option<usize>,
//     max: Option<usize>,
// }

struct StructField {
    name: syn::Ident,
    ty: syn::Type,
    arguments: Vec<syn::Ident>,
    //vector_size: Option<VectorSize>,
}

// fn extract_vector_size_argument(arg: &NestedMeta) -> Option<usize> {
//     match arg {
//         NestedMeta::Meta(meta) => match meta {
//             syn::Meta::Path(path) => {
//                 if let Some(ident) = path.get_ident() {
//                     if ident == "_" {
//                         return None;
//                     }
//                 }
//                 panic!("Invalid vector_size format (Argument must be a number or _)");
//             }
//             _ => panic!("Invalid vector_size format (Invalid argument meta type)"),
//         },
//         NestedMeta::Lit(literal) => match literal {
//             syn::Lit::Int(number) => Some(number.base10_parse::<usize>().unwrap()),
//             _ => panic!("Invalid vector_size format (Invalid argument literal type)"),
//         },
//     }
// }

fn parse_fields(fields: &Fields) -> Vec<StructField> {
    let mut result = Vec::<StructField>::new();
    for field in fields {
        let name = field.ident.clone().unwrap();
        let ty = field.ty.clone();

        let mut arguments = Vec::<syn::Ident>::new();
        //let mut vector_size = None;
        let mut field_args_found = false;
        //let mut vector_size_found = false;
        for attr in &field.attrs {
            if *attr
                .path
                .get_ident()
                .expect("Expecting attribute identifier")
                == "field_args"
            {
                if field_args_found {
                    panic!("Attribute field_args is alterady specified")
                }
                field_args_found = true;
                let list = match attr.parse_meta().unwrap() {
                    syn::Meta::List(list) => list,
                    _ => panic!("Incorect format (Not MetaList)"),
                };
                for entry in list.nested {
                    let meta = match entry {
                        syn::NestedMeta::Meta(meta) => meta,
                        _ => panic!("Incorect format (Not Meta)"),
                    };
                    let ident: syn::Ident = match meta {
                        syn::Meta::Path(path) => path
                            .get_ident()
                            .expect("Path does not contain Identifier")
                            .clone(),
                        _ => panic!("Incorect format (NestedMeta::Meta is Not Path)"),
                    };
                    arguments.push(ident);
                }
            }
            // "vector_size" => {
            //     if vector_size_found {
            //         panic!("Attribute vector_size is already defined");
            //     }
            //     vector_size_found = true;
            //     let list = match attr.parse_meta().unwrap() {
            //         syn::Meta::List(list) => list,
            //         _ => panic!("Inalid vector_size format (Not MetaList)"),
            //     };
            //     let (min, max) = if list.nested.len() == 1 {
            //         let arg = extract_vector_size_argument(list.nested.first().unwrap());
            //         (arg, arg)
            //     } else if list.nested.len() == 2 {
            //         let min = extract_vector_size_argument(list.nested.first().unwrap());
            //         let max = extract_vector_size_argument(list.nested.last().unwrap());
            //         (min, max)
            //     } else {
            //         panic!("Invalid vector_size format (Wrong arguments count)")
            //     };

            //     if min.is_some() && max.is_some() && min.unwrap() > max.unwrap() {
            //         panic!("Invalid vector_size format (first argument cannot be greater than second)");
            //     }
            //     vector_size = Some(VectorSize { min, max });
            // }
        }
        result.push(StructField {
            name,
            ty,
            arguments,
            //vector_size,
        });
    }
    result
}

enum PathPart {
    Path(Vec<proc_macro2::Ident>),
    TemplateArgs(Vec<syn::GenericArgument>),
}

impl ToTokens for PathPart {
    fn to_tokens(&self, tokens: &mut syn::__private::TokenStream2) {
        match &self {
            PathPart::Path(path) => {
                for (index, ident) in path.iter().enumerate() {
                    if index > 0 {
                        tokens.append(Punct::new(':', Spacing::Joint));
                        tokens.append(Punct::new(':', Spacing::Alone));
                    }
                    ident.to_tokens(tokens);
                }
            }
            PathPart::TemplateArgs(args) => {
                tokens.append(Punct::new('<', Spacing::Alone));
                for (index, argument) in args.iter().enumerate() {
                    if index > 0 {
                        tokens.append(Punct::new(',', Spacing::Alone));
                    }
                    argument.to_tokens(tokens);
                }
                tokens.append(Punct::new('>', Spacing::Alone));
            }
        }
    }
}

enum Type {
    Path(Vec<PathPart>),
    Array(syn::TypeArray),
}

impl ToTokens for Type {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        match &self {
            Type::Path(path) => {
                for (index, path) in path.iter().enumerate() {
                    if index > 0 {
                        tokens.append(Punct::new(':', Spacing::Joint));
                        tokens.append(Punct::new(':', Spacing::Alone));
                    }
                    path.to_tokens(tokens);
                }
            }
            Type::Array(array) => {
                tokens.append(Punct::new('<', Spacing::Alone));
                array.to_tokens(tokens);
                tokens.append(Punct::new('>', Spacing::Alone));
            }
        }
    }
}

fn parse_type(ty: &syn::Type) -> Type {
    match ty {
        syn::Type::Path(path) => {
            let mut result = Vec::<PathPart>::new();
            let mut current_path = Vec::<proc_macro2::Ident>::new();
            for x in &path.path.segments {
                let ident = &x.ident;
                current_path.push(ident.clone());
                let arguments = match &x.arguments {
                    syn::PathArguments::None => continue,
                    syn::PathArguments::AngleBracketed(a) => a,
                    syn::PathArguments::Parenthesized(_) => {
                        panic!("PathArguments::Parenthesized is not supported")
                    }
                };

                result.push(PathPart::Path(current_path));
                current_path = Vec::<proc_macro2::Ident>::new();
                let arguments: Vec<syn::GenericArgument> = arguments.args.iter().cloned().collect();
                result.push(PathPart::TemplateArgs(arguments));
            }
            if !current_path.is_empty() {
                result.push(PathPart::Path(current_path));
            }
            Type::Path(result)
        }
        syn::Type::Array(array) => Type::Array(array.clone()),
        syn::Type::BareFn(_) => panic!("Unexpected type syn::Type::BareFn"),
        syn::Type::Group(_) => panic!("Unexpected type syn::Type::Group"),
        syn::Type::ImplTrait(_) => panic!("Unexpected type syn::Type::ImplTrait"),
        syn::Type::Infer(_) => panic!("Unexpected type syn::Type::Infer"),
        syn::Type::Macro(_) => panic!("Unexpected type syn::Type::Macro"),
        syn::Type::Never(_) => panic!("Unexpected type syn::Type::ArrNeveray"),
        syn::Type::Paren(_) => panic!("Unexpected type syn::Type::Paren"),
        syn::Type::Ptr(_) => panic!("Unexpected type syn::Type::ArrPtray"),
        syn::Type::Reference(_) => panic!("Unexpected type syn::Type::Reference"),
        syn::Type::Slice(_) => panic!("Unexpected type syn::Type::Slice"),
        syn::Type::TraitObject(_) => {
            panic!("Unexpected type syn::Type::TraitObject")
        }
        syn::Type::Tuple(_) => panic!("Unexpected type syn::Type::Tuple"),
        syn::Type::Verbatim(_) => panic!("Unexpected type syn::Type::Verbatim"),
        _ => panic!("Unexpected type"),
    }
}

fn extract_path(entry: NestedMeta) -> syn::Path {
    let meta = match entry {
        syn::NestedMeta::Meta(meta) => meta,
        _ => panic!("Incorect format (Not Meta)"),
    };
    match meta {
        syn::Meta::Path(path) => path,
        _ => panic!("Incorect format (NestedMeta::Meta is Not Path)"),
    }
}

fn extract_ident(entry: NestedMeta) -> syn::Ident {
    let path = extract_path(entry);
    let ident = path.get_ident().unwrap_or_else(|| {
        let path = quote!(#path);
        panic!("Unable to extract ident from {path}");
    });
    ident.clone()
}

enum StructureType {
    //Fragment,
    Record(syn::Path),
    Struct,
}

type Verifier = syn::Path;

fn parse_structure_attributes(attributes: &[Attribute]) -> (StructureType, Option<Verifier>) {
    let mut structure_type: Option<StructureType> = None;
    let mut verifier: Option<Verifier> = None;

    for attr in attributes {
        let p = attr
            .path
            .get_ident()
            .expect("Attribute identifier expected");
        if *p != "from_record" {
            continue;
        }
        if structure_type.is_some() {
            panic!("Attribute from_record is already defined");
        }
        let mut list = match attr.parse_meta().unwrap() {
            syn::Meta::List(list) => list.nested.into_iter(),
            _ => panic!("Incorect format (Not MetaList)"),
        };
        let structure_type_ident = extract_ident(
            list.next()
                .expect("Expecting structure type (Record or Struct)"),
        );
        structure_type = match structure_type_ident.to_string().as_str() {
            //"Fragment" => Some(StructureType::Fragment),
            "Record" => {
                let path = extract_path(list.next().expect("Expecting RecordType"));
                Some(StructureType::Record(path))
            }
            "Struct" => Some(StructureType::Struct),
            other => panic!("Unexpected structure type {other}"),
        };
        if let Some(entry) = list.next() {
            verifier = Some(extract_path(entry));
            if list.next().is_some() {
                panic!("Unexpected number of arguments");
            }
        }
    }
    (
        structure_type.expect("Attribute from_record is not defined"),
        verifier,
    )
}

fn impl_macro(ast: &syn::DeriveInput) -> proc_macro::TokenStream {
    match &ast.data {
        syn::Data::Struct(data) => parse_data_struct(&ast.ident, &ast.attrs, data),
        //syn::Data::Enum(data) => parse_data_enum(&ast.ident, &ast.attrs, data),
        _ => panic!("Unsupported structure type"),
    }
}

fn parse_data_struct(
    structure_name: &syn::Ident,
    attributes: &[Attribute],
    data: &DataStruct,
) -> proc_macro::TokenStream {
    let (structure_type, verifier) = parse_structure_attributes(attributes);
    let fields = parse_fields(&data.fields);
    //let is_fragment = matches!(&structure_type, StructureType::Fragment);
    let inner = {
        let mut inner = quote!();
        let mut result_arguments = quote!();

        for (index, field) in fields.iter().enumerate() {
            let field_name = &field.name;
            let ty = parse_type(&field.ty);

            let mut func_args = quote!();
            for arg in &field.arguments {
                func_args = quote!(#func_args, &#arg)
            }

            //let mut create_default_inner = true;
            // if is_fragment {
            //     let mut invalid_vector_size_usage = field.vector_size.is_some();
            //     if let Type::Path(path) = &ty {
            //         match (path.get(0), path.get(1)) {
            //             (Some(PathPart::Path(path)), Some(PathPart::TemplateArgs(args)))
            //                 if path.len() == 1 && args.len() == 1 =>
            //             {
            //                 let ty = path.first().unwrap();
            //                 if ty == "Option" {
            //                     let inner_type = args.first().unwrap();
            //                     inner = quote!(
            //                         #inner
            //                         let #field_name = if #inner_type::accepted_recod_types().contains(&stream.ty) {
            //                             let #field_name = #inner_type::from_record(stream, anomalies #func_args)?;
            //                             Some(#field_name)
            //                         }
            //                         else {
            //                             debug!("skipping {} {stream:?}", stringify!(#inner_type));
            //                             None
            //                         };
            //                     );
            //                     create_default_inner = false;
            //                 } else if ty == "Vec" {
            //                     let vector_size = field.vector_size.as_ref().expect("vector_size atribute MUST be defined for Vec<T> inside Fragment");
            //                     let min = vector_size.min.unwrap_or(0);
            //                     let inner_type = args.first().unwrap();

            //                     inner = if min > 0 {
            //                         quote!(
            //                             #inner
            //                             let mut #field_name = Vec::<#inner_type>::new();
            //                             while #field_name.len() < #min {
            //                                 if !#inner_type::accepted_recod_types().contains(&stream.ty) {
            //                                     return Err(io::Error::new(
            //                                         io::ErrorKind::InvalidData,
            //                                         format!(
            //                                             "Found unexpected stream type {:x?}",
            //                                             stream.ty,
            //                                         ),
            //                                     ));
            //                                 }
            //                                 let tmp = #inner_type::from_record(stream, anomalies #func_args)?;
            //                                 #field_name.push(tmp);
            //                             }
            //                         )
            //                     } else {
            //                         quote!(
            //                             #inner
            //                             let mut #field_name = Vec::<#inner_type>::new();
            //                         )
            //                     };

            //                     let loop_def = match vector_size.max {
            //                         Some(max) if max == min => quote!(),
            //                         Some(max) => quote!(while #field_name.len() < #max),
            //                         None => quote!(loop),
            //                     };
            //                     if !loop_def.is_empty() {
            //                         inner = quote!(
            //                             #inner
            //                             #loop_def {
            //                                 if !#inner_type::accepted_recod_types().contains(&stream.ty) {
            //                                     break;
            //                                 }
            //                                 let tmp = #inner_type::from_record(stream, anomalies #func_args)?;
            //                                 #field_name.push(tmp);
            //                             }
            //                         );
            //                     }

            //                     invalid_vector_size_usage = false;
            //                     create_default_inner = false;
            //                 }
            //             }
            //             _ => {}
            //         }
            //     }
            //     if invalid_vector_size_usage {
            //         panic!("vector_size atribute can be used only for Vec<T>")
            //     }
            // } else if field.vector_size.is_some() {
            //     panic!("vector_size atribute can be used only in Fragment");
            // }
            //if create_default_inner {
            let ty = quote!(#ty);
            inner = quote!(
                #inner
                let #field_name = #ty::from_record(stream, anomalies #func_args)?;
            );
            //}
            if index > 0 {
                result_arguments = quote!(#result_arguments ,);
            }
            result_arguments = quote!(#result_arguments #field_name)
        }
        inner = quote!(
            #inner
            let result = Self { #result_arguments };
        );
        inner
    };

    let record_type_check = if let StructureType::Record(record_type) = &structure_type {
        quote!(
                if stream.ty != #record_type {
                return Err(
                    format!(
                        "Found unexpected stream type {:x?}; should be {:x?}",
                        stream.ty, #record_type
                    ).into()
                );
            }
        )
    } else {
        quote!()
    };

    let verifier_call = if let Some(verifier) = &verifier {
        quote!(
            let mut result_anomalies: Anomalies = #verifier (&result);
            anomalies.append(&mut result_anomalies);
        )
    } else {
        quote!()
    };

    // let next_record = if let StructureType::Record(stream) = &structure_type {
    //     let record_type = &stream.segments.last().unwrap().ident;
    //     if record_type == "EOF" {
    //         quote!()
    //     } else {
    //         quote!(stream.next()?;)
    //     }
    // } else {
    //     quote!()
    // };

    let impl_from_record = quote! {
        impl FromRecordStream for #structure_name {
            fn from_record<R: Read + Seek>(stream: &mut RecordStream<R>, anomalies: &mut Anomalies) -> Result<Self, ExcelError>
            where
                Self: Sized {
                    debug!("{}::from_record({stream:?})", stringify!(#structure_name));
                    #record_type_check
                    #inner
                    #verifier_call
                    //#next_record
                    Ok(result)
                }
        }
    };

    let impl_record_type = match &structure_type {
        // StructureType::Fragment => {
        //     let mut types = Vec::<syn::Type>::new();
        //     for field in &fields {
        //         let ty = parse_type(&field.ty);
        //         match ty {
        //             Type::Path(path) => match (path.get(0), path.get(1)) {
        //                 (Some(PathPart::Path(path)), Some(PathPart::TemplateArgs(args)))
        //                     if path.len() == 1 && args.len() == 1 =>
        //                 {
        //                     let ty = path.first().unwrap();
        //                     if ty == "Option" {
        //                         let inner_type = match args.first().unwrap() {
        //                             syn::GenericArgument::Type(ty) => ty,
        //                             _ => panic!("Unsupported"),
        //                         };
        //                         types.push(inner_type.clone());
        //                     } else if ty == "Vec" {
        //                         let vector_size = field.vector_size.as_ref().unwrap();
        //                         let min = vector_size.min.unwrap_or(0);
        //                         let inner_type = match args.first().unwrap() {
        //                             syn::GenericArgument::Type(ty) => ty,
        //                             _ => panic!("Unsupported"),
        //                         };
        //                         types.push(inner_type.clone());
        //                         if min > 0 {
        //                             break;
        //                         }
        //                     }
        //                 }
        //                 _ => {
        //                     types.push(field.ty.clone());
        //                     break;
        //                 }
        //             },
        //             Type::Array(_) => {
        //                 todo!(
        //                     "accepted_record_types is not implemented for arrays inside Fragment"
        //                 );
        //             }
        //         };
        //     }

        //     if types.is_empty() {
        //         quote!(&[])
        //     } else if types.len() == 1 {
        //         let ty = types.first().unwrap();
        //         quote!(#ty::accepted_recod_types())
        //     } else {
        //         let mut accepted_recod_types_inner = quote!();
        //         for (index, data_type) in types.iter().enumerate() {
        //             let separator = if index > 0 { quote!(,) } else { quote!() };
        //             accepted_recod_types_inner = quote!(
        //                 #accepted_recod_types_inner #separator #data_type::accepted_recod_types()
        //             )
        //         }

        //         quote!(
        //             static RESULT: Lazy<Vec<RecordType>> = Lazy::new(|| {
        //                 let mut result = Vec::<RecordType>::new();
        //                 for entry in [#accepted_recod_types_inner] {
        //                     for ty in entry {
        //                         if !result.contains(ty) {
        //                             result.push(*ty)
        //                         }
        //                     }
        //                 }
        //                 result
        //             });
        //             &RESULT
        //         )
        //     }
        // }
        StructureType::Record(record_type) => quote!(#record_type),
        StructureType::Struct => quote!(),
    };

    let impl_record_types = if impl_record_type.is_empty() {
        quote!()
    } else {
        quote!(
            impl Record for #structure_name {
                fn record_type() ->RecordType {
                    #impl_record_type
                }
            }
        )
    };

    let gen = quote!(
        #impl_from_record
        #impl_record_types
    );

    gen.into()
}

// struct EnumVariant {
//     variant_name: syn::Ident,
//     data_type: syn::Type,
// }

// fn parse_data_enum(
//     structure_name: &syn::Ident,
//     attributes: &[Attribute],
//     data: &DataEnum,
// ) -> proc_macro::TokenStream {
//     let (structure_type, verifier) = parse_structure_attributes(attributes);
//     if !matches!(structure_type, StructureType::Fragment) {
//         panic!("FromRecordStream derive macro supports only 'Fragment' enums");
//     }
//     if verifier.is_some() {
//         panic!("enum does not support verifier");
//     }

//     let mut variants = Vec::<EnumVariant>::new();
//     for entry in data.variants.iter() {
//         let fields = match &entry.fields {
//             Fields::Unnamed(fields) => &fields.unnamed,
//             _ => panic!("Every enum variant must one element tuple"),
//         };
//         if fields.len() != 1 {
//             panic!("Every enum variant must one element tuple")
//         }

//         let variant_name = entry.ident.clone();
//         let data_type: syn::Type = fields.first().unwrap().ty.clone();

//         variants.push(EnumVariant {
//             variant_name,
//             data_type,
//         });
//     }

//     let mut from_record_inner = quote!();
//     let mut accepted_recod_types_inner = quote!();

//     if variants.is_empty() {
//         from_record_inner = quote!(Ok(Self));
//         accepted_recod_types_inner = quote!(&[]);
//     } else if variants.len() == 1 {
//         let variant = variants.first().unwrap();
//         let variant_name = &variant.variant_name;
//         let data_type = &variant.data_type;

//         from_record_inner = quote!(
//             let val = #data_type::from_record(stream, anomalies)?;
//             Ok(Self::#variant_name(val))
//         );
//         accepted_recod_types_inner = quote!(
//             #data_type::accepted_recod_types()
//         );
//     } else {
//         for (index, variant) in variants.iter().enumerate() {
//             let variant_name = &variant.variant_name;
//             let data_type = &variant.data_type;
//             let condition = if index == 0 {
//                 quote!(if #data_type::accepted_recod_types().contains(&stream.ty))
//             } else if index >= variants.len() - 1 {
//                 quote!(else)
//             } else {
//                 quote!(else if #data_type::accepted_recod_types().contains(&stream.ty))
//             };
//             from_record_inner = quote!(
//                 #from_record_inner
//                 #condition {
//                     Ok(Self::#variant_name(#data_type::from_record(stream, anomalies)?))
//                 }
//             );
//             let separator = if index > 0 { quote!(,) } else { quote!() };
//             accepted_recod_types_inner = quote!(
//                 #accepted_recod_types_inner #separator #data_type::accepted_recod_types()
//             )
//         }
//         accepted_recod_types_inner = quote!(
//             static RESULT: Lazy<Vec<RecordType>> = Lazy::new(|| {
//                 let mut result = Vec::<RecordType>::new();
//                 for entry in [#accepted_recod_types_inner] {
//                     for ty in entry {
//                         if !result.contains(ty) {
//                             result.push(*ty)
//                         }
//                     }
//                 }
//                 result
//             });
//             &RESULT
//         )
//     }

//     let impl_from_record = quote! {
//         impl FromRecordStream for #structure_name {
//             fn from_record<R: Read + Seek>(stream: &mut Record<R>, anomalies: &mut Anomalies) -> Result<Self, ExcelError>
//             where
//                 Self: Sized {
//                     debug!("{}::from_record({stream:?})", stringify!(#structure_name));
//                     #from_record_inner
//                 }
//         }
//     };

//     let impl_accepted_record_types = quote!(
//         impl AcceptedRecordTypes for #structure_name {
//             fn accepted_recod_types() -> &'static [RecordType] {
//                 #accepted_recod_types_inner
//             }
//         }
//     );

//     let gen = quote!(
//         #impl_from_record
//         #impl_accepted_record_types
//     );

//     gen.into()
// }
