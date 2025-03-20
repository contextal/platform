use proc_macro2::{Punct, Spacing};
use quote::{ToTokens, TokenStreamExt, quote};
use syn::{self, Attribute, DataStruct, Fields};

#[proc_macro_derive(FromRecordStream, attributes(from_record))]
pub fn from_record_dervive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: syn::DeriveInput = syn::parse(input).unwrap();
    impl_macro(&ast)
}

struct StructField {
    name: syn::Ident,
    ty: syn::Type,
}

fn parse_fields(fields: &Fields) -> Vec<StructField> {
    fields
        .into_iter()
        .map(|field| StructField {
            name: field.ident.clone().unwrap(),
            ty: field.ty.clone(),
        })
        .collect()
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

enum StructureType {
    Record(syn::Path),
    Struct,
}

fn parse_structure_attributes(attributes: &[Attribute]) -> StructureType {
    let mut structure_type: Option<StructureType> = None;
    for attr in attributes {
        if !attr.path().is_ident("from_record") {
            continue;
        }
        if structure_type.is_some() {
            panic!("Attribute from_record is already defined");
        }
        let list: syn::punctuated::Punctuated<syn::Path, syn::Token![,]> = attr
            .parse_args_with(syn::punctuated::Punctuated::parse_terminated)
            .expect("aguments to from_record to be a list of idents");
        let mut it = list.iter();
        structure_type = Some(match it.next() {
            Some(path) if path.is_ident("Record") => {
                let rtype_path = it.next().expect("RecordType");
                StructureType::Record(rtype_path.clone())
            }
            Some(path) if path.is_ident("Struct") => StructureType::Struct,
            _ => panic!("expecting structure type (Record or Struct)"),
        });
    }
    structure_type.expect("Attribute from_record is not defined")
}

fn impl_macro(ast: &syn::DeriveInput) -> proc_macro::TokenStream {
    match &ast.data {
        syn::Data::Struct(data) => parse_data_struct(&ast.ident, &ast.attrs, data),
        _ => panic!("Unsupported structure type"),
    }
}

fn parse_data_struct(
    structure_name: &syn::Ident,
    attributes: &[Attribute],
    data: &DataStruct,
) -> proc_macro::TokenStream {
    let structure_type = parse_structure_attributes(attributes);
    let fields = parse_fields(&data.fields);
    let inner = {
        let mut inner = quote!();
        let mut result_arguments = quote!();

        for (index, field) in fields.iter().enumerate() {
            let field_name = &field.name;
            let ty = parse_type(&field.ty);
            let ty = quote!(#ty);
            inner = quote!(
                #inner
                let #field_name = #ty::from_record(stream, anomalies)?;
            );
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

    let impl_from_record = quote! {
        impl FromRecordStream for #structure_name {
            fn from_record<R: Read + Seek>(stream: &mut RecordStream<R>, anomalies: &mut Anomalies) -> Result<Self, ExcelError>
            where
                Self: Sized {
                    debug!("{}::from_record({stream:?})", stringify!(#structure_name));
                    #record_type_check
                    #inner
                    Ok(result)
                }
        }
    };

    let impl_record_type = match &structure_type {
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

    let ts = quote!(
        #impl_from_record
        #impl_record_types
    );

    ts.into()
}
