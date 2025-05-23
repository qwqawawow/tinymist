use core::fmt;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, OnceLock};

use ecow::{eco_format, EcoString};
use serde::{Deserialize, Serialize};

use super::tidy::*;
use crate::syntax::DeclExpr;
use crate::ty::{Interned, ParamAttrs, ParamTy, StrRef, Ty, TypeVarBounds};
use crate::upstream::plain_docs_sentence;

/// The documentation string of an item
#[derive(Debug, Clone, Default)]
pub struct DocString {
    /// The documentation of the item
    pub docs: Option<EcoString>,
    /// The typing on definitions
    pub var_bounds: HashMap<DeclExpr, TypeVarBounds>,
    /// The variable doc associated with the item
    pub vars: BTreeMap<StrRef, VarDoc>,
    /// The type of the resultant type
    pub res_ty: Option<Ty>,
}

impl DocString {
    /// Gets the docstring as a variable doc
    pub fn as_var(&self) -> VarDoc {
        VarDoc {
            docs: self.docs.clone().unwrap_or_default(),
            ty: self.res_ty.clone(),
        }
    }

    /// Get the documentation of a variable associated with the item
    pub fn get_var(&self, name: &StrRef) -> Option<&VarDoc> {
        self.vars.get(name)
    }

    /// Get the type of a variable associated with the item
    pub fn var_ty(&self, name: &StrRef) -> Option<&Ty> {
        self.get_var(name).and_then(|v| v.ty.as_ref())
    }
}

/// The documentation string of a variable associated with some item.
#[derive(Debug, Clone, Default)]
pub struct VarDoc {
    /// The documentation of the variable
    pub docs: EcoString,
    /// The type of the variable
    pub ty: Option<Ty>,
}

impl VarDoc {
    /// Convert the variable doc to an untyped version
    pub fn to_untyped(&self) -> Arc<UntypedDefDocs> {
        Arc::new(UntypedDefDocs::Variable(VarDocsT {
            docs: self.docs.clone(),
            return_ty: (),
            def_docs: OnceLock::new(),
        }))
    }
}

type TypeRepr = Option<(
    /* short */ EcoString,
    /* long */ EcoString,
    /* value */ EcoString,
)>;

/// Documentation about a definition (without type information).
pub type UntypedDefDocs = DefDocsT<()>;
/// Documentation about a definition.
pub type DefDocs = DefDocsT<TypeRepr>;

/// Documentation about a definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum DefDocsT<T> {
    /// Documentation about a function.
    #[serde(rename = "func")]
    Function(Box<SignatureDocsT<T>>),
    /// Documentation about a variable.
    #[serde(rename = "var")]
    Variable(VarDocsT<T>),
    /// Documentation about a module.
    #[serde(rename = "module")]
    Module(TidyModuleDocs),
    /// Other kinds of documentation.
    #[serde(rename = "plain")]
    Plain {
        /// The content of the documentation.
        docs: EcoString,
    },
}

impl<T> DefDocsT<T> {
    /// Get the markdown representation of the documentation.
    pub fn docs(&self) -> &EcoString {
        match self {
            Self::Function(docs) => &docs.docs,
            Self::Variable(docs) => &docs.docs,
            Self::Module(docs) => &docs.docs,
            Self::Plain { docs } => docs,
        }
    }
}

impl DefDocs {
    /// Get full documentation for the signature.
    pub fn hover_docs(&self) -> EcoString {
        match self {
            DefDocs::Function(docs) => docs.hover_docs().clone(),
            _ => plain_docs_sentence(self.docs()),
        }
    }
}

/// Describes a primary function signature.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureDocsT<T> {
    /// Documentation for the function.
    pub docs: EcoString,
    /// The positional parameters.
    pub pos: Vec<ParamDocsT<T>>,
    /// The named parameters.
    pub named: BTreeMap<Interned<str>, ParamDocsT<T>>,
    /// The rest parameter.
    pub rest: Option<ParamDocsT<T>>,
    /// The return type.
    pub ret_ty: T,
    /// The full documentation for the signature.
    #[serde(skip)]
    pub hover_docs: OnceLock<EcoString>,
}

impl SignatureDocsT<TypeRepr> {
    /// Get full documentation for the signature.
    pub fn hover_docs(&self) -> &EcoString {
        self.hover_docs
            .get_or_init(|| plain_docs_sentence(&format!("{}", SigHoverDocs(self))))
    }
}

struct SigHoverDocs<'a>(&'a SignatureDocs);

impl fmt::Display for SigHoverDocs<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let docs = self.0;
        let base_docs = docs.docs.trim();

        if !base_docs.is_empty() {
            f.write_str(base_docs)?;
        }

        fn write_param_docs(
            f: &mut fmt::Formatter<'_>,
            docs: &ParamDocsT<TypeRepr>,
            kind: &str,
            is_first: &mut bool,
        ) -> fmt::Result {
            if *is_first {
                *is_first = false;
                write!(f, "\n\n## {}\n\n", docs.name)?;
            } else {
                write!(f, "\n\n## {} ({kind})\n\n", docs.name)?;
            }

            // p.cano_type.0
            if let Some(t) = &docs.cano_type {
                write!(f, "```typc\ntype: {}\n```\n\n", t.2)?;
            }

            f.write_str(docs.docs.trim())?;

            Ok(())
        }

        if !docs.pos.is_empty() {
            f.write_str("\n\n# Positional Parameters")?;

            let mut is_first = true;
            for pos_docs in &docs.pos {
                write_param_docs(f, pos_docs, "positional", &mut is_first)?;
            }
        }

        if docs.rest.is_some() {
            f.write_str("\n\n# Rest Parameters")?;

            let mut is_first = true;
            if let Some(rest) = &docs.rest {
                write_param_docs(f, rest, "spread right", &mut is_first)?;
            }
        }

        if !docs.named.is_empty() {
            f.write_str("\n\n# Named Parameters")?;

            let mut is_first = true;
            for named_docs in docs.named.values() {
                write_param_docs(f, named_docs, "named", &mut is_first)?;
            }
        }

        Ok(())
    }
}

/// Documentation about a signature.
pub type UntypedSignatureDocs = SignatureDocsT<()>;
/// Documentation about a signature.
pub type SignatureDocs = SignatureDocsT<TypeRepr>;

impl SignatureDocs {
    /// Get the markdown representation of the documentation.
    pub fn print(&self, f: &mut impl std::fmt::Write) -> fmt::Result {
        let mut is_first = true;
        let mut write_sep = |f: &mut dyn std::fmt::Write| {
            if is_first {
                is_first = false;
                return f.write_str("\n  ");
            }
            f.write_str(",\n  ")
        };

        f.write_char('(')?;
        for pos_docs in &self.pos {
            write_sep(f)?;
            f.write_str(&pos_docs.name)?;
            if let Some(t) = &pos_docs.cano_type {
                write!(f, ": {}", t.0)?;
            }
        }
        if let Some(rest) = &self.rest {
            write_sep(f)?;
            f.write_str("..")?;
            f.write_str(&rest.name)?;
            if let Some(t) = &rest.cano_type {
                write!(f, ": {}", t.0)?;
            }
        }

        if !self.named.is_empty() {
            let mut name_prints = vec![];
            for v in self.named.values() {
                let ty = v.cano_type.as_ref().map(|t| &t.0);
                name_prints.push((v.name.clone(), ty, v.default.clone()))
            }
            name_prints.sort();
            for (name, ty, val) in name_prints {
                write_sep(f)?;
                let val = val.as_deref().unwrap_or("any");
                let mut default = val.trim();
                if default.starts_with('{') && default.ends_with('}') && default.len() > 30 {
                    default = "{ .. }"
                }
                if default.starts_with('`') && default.ends_with('`') && default.len() > 30 {
                    default = "raw"
                }
                if default.starts_with('[') && default.ends_with(']') && default.len() > 30 {
                    default = "content"
                }
                f.write_str(&name)?;
                if let Some(ty) = ty {
                    write!(f, ": {ty}")?;
                }
                if default.contains('\n') {
                    write!(f, " = {}", default.replace("\n", "\n  "))?;
                } else {
                    write!(f, " = {default}")?;
                }
            }
        }
        if !is_first {
            f.write_str(",\n")?;
        }
        f.write_char(')')?;

        Ok(())
    }
}

/// Documentation about a variable (without type information).
pub type UntypedVarDocs = VarDocsT<()>;
/// Documentation about a variable.
pub type VarDocs = VarDocsT<Option<(EcoString, EcoString, EcoString)>>;

/// Describes a primary pattern binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarDocsT<T> {
    /// Documentation for the pattern binding.
    pub docs: EcoString,
    /// The inferred type of the pattern binding source.
    pub return_ty: T,
    /// Cached documentation for the definition.
    #[serde(skip)]
    pub def_docs: OnceLock<String>,
}

impl VarDocs {
    /// Get the markdown representation of the documentation.
    pub fn def_docs(&self) -> &String {
        self.def_docs
            .get_or_init(|| plain_docs_sentence(&self.docs).into())
    }
}

/// Documentation about a parameter (without type information).
pub type TypelessParamDocs = ParamDocsT<()>;
/// Documentation about a parameter.
pub type ParamDocs = ParamDocsT<TypeRepr>;

/// Describes a function parameter.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ParamDocsT<T> {
    /// The parameter's name.
    pub name: Interned<str>,
    /// Documentation for the parameter.
    pub docs: EcoString,
    /// Inferred type of the parameter.
    pub cano_type: T,
    /// The parameter's default name as value.
    pub default: Option<EcoString>,
    /// The attribute of the parameter.
    #[serde(flatten)]
    pub attrs: ParamAttrs,
}

impl ParamDocs {
    pub fn new(param: &ParamTy, ty: Option<&Ty>) -> Self {
        Self {
            name: param.name.as_ref().into(),
            docs: param.docs.clone().unwrap_or_default(),
            cano_type: format_ty(ty.or(Some(&param.ty))),
            default: param.default.clone(),
            attrs: param.attrs,
        }
    }
}

pub fn format_ty(ty: Option<&Ty>) -> TypeRepr {
    let ty = ty?;
    let short = ty.repr().unwrap_or_else(|| "any".into());
    let long = eco_format!("{ty:?}");
    let value = ty.value_repr().unwrap_or_else(|| "".into());

    Some((short, long, value))
}
