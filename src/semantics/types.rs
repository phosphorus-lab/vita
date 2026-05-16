//! Type system definitions for the Vita language.

use std::collections::HashMap;
use std::fmt;

/// A resolved type in the Vita type system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    // Primitives
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F16,
    F32,
    F64,
    F128,
    Bool,
    Char,
    Str,

    // Named types (def, enum)
    Named(String),

    // Generic instantiation
    Generic { name: String, args: Vec<Type> },

    // Fixed-size array
    Array { element: Box<Type>, size: usize },

    // Dynamic array
    DynArray(Box<Type>),

    // Tuple
    Tuple(Vec<Type>),

    // Map
    Map { key: Box<Type>, value: Box<Type> },

    // Set
    Set(Box<Type>),

    // Function
    Fn { params: Vec<Type>, ret: Box<Type> },

    // Option
    Option(Box<Type>),

    // Result
    Result { ok: Box<Type>, err: Box<Type> },

    // Unit
    Unit,

    // Type variable (for generics during checking)
    Var(String),

    // Unresolved / error type
    Unknown,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::I8 => write!(f, "i8"),
            Type::I16 => write!(f, "i16"),
            Type::I32 => write!(f, "i32"),
            Type::I64 => write!(f, "i64"),
            Type::I128 => write!(f, "i128"),
            Type::U8 => write!(f, "u8"),
            Type::U16 => write!(f, "u16"),
            Type::U32 => write!(f, "u32"),
            Type::U64 => write!(f, "u64"),
            Type::U128 => write!(f, "u128"),
            Type::F16 => write!(f, "f16"),
            Type::F32 => write!(f, "f32"),
            Type::F64 => write!(f, "f64"),
            Type::F128 => write!(f, "f128"),
            Type::Bool => write!(f, "bool"),
            Type::Char => write!(f, "char"),
            Type::Str => write!(f, "str"),
            Type::Named(n) => write!(f, "{}", n),
            Type::Generic { name, args } => {
                write!(f, "{}<", name)?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", a)?;
                }
                write!(f, ">")
            }
            Type::Array { element, size } => write!(f, "[{}; {}]", element, size),
            Type::DynArray(elem) => write!(f, "Array<{}>", elem),
            Type::Tuple(types) => {
                write!(f, "(")?;
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", t)?;
                }
                write!(f, ")")
            }
            Type::Map { key, value } => write!(f, "Map<{}, {}>", key, value),
            Type::Set(elem) => write!(f, "Set<{}>", elem),
            Type::Fn { params, ret } => {
                write!(f, "fn(")?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, ") -> {}", ret)
            }
            Type::Option(inner) => write!(f, "Option<{}>", inner),
            Type::Result { ok, err } => write!(f, "Result<{}, {}>", ok, err),
            Type::Unit => write!(f, "()"),
            Type::Var(v) => write!(f, "{}", v),
            Type::Unknown => write!(f, "?"),
        }
    }
}

impl Type {
    /// Get the LLVM IR type name for this type.
    pub fn llvm_type(&self) -> &str {
        match self {
            Type::I8 => "i8",
            Type::I16 => "i16",
            Type::I32 => "i32",
            Type::I64 => "i64",
            Type::I128 => "i128",
            Type::U8 => "i8",
            Type::U16 => "i16",
            Type::U32 => "i32",
            Type::U64 => "i64",
            Type::U128 => "i128",
            Type::F16 => "half",
            Type::F32 => "float",
            Type::F64 => "double",
            Type::F128 => "fp128",
            Type::Bool => "i1",
            Type::Char => "i32",
            Type::Unit => "void",
            Type::Str => "ptr",
            _ => "ptr",
        }
    }

    /// Is this a signed integer type?
    pub fn is_signed_int(&self) -> bool {
        matches!(
            self,
            Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::I128
        )
    }

    /// Is this an unsigned integer type?
    pub fn is_unsigned_int(&self) -> bool {
        matches!(
            self,
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128
        )
    }

    /// Is this a float type?
    pub fn is_float(&self) -> bool {
        matches!(self, Type::F16 | Type::F32 | Type::F64 | Type::F128)
    }

    /// Is this a numeric type?
    pub fn is_numeric(&self) -> bool {
        self.is_signed_int() || self.is_unsigned_int() || self.is_float()
    }

    /// Is this a primitive type?
    pub fn is_primitive(&self) -> bool {
        self.is_numeric() || matches!(self, Type::Bool | Type::Char)
    }

    /// Size in bytes for LLVM.
    pub fn llvm_size(&self) -> u32 {
        match self {
            Type::I8 | Type::U8 => 1,
            Type::I16 | Type::U16 => 2,
            Type::I32 | Type::U32 | Type::F32 => 4,
            Type::I64 | Type::U64 | Type::F64 => 8,
            Type::I128 | Type::U128 | Type::F128 => 16,
            Type::Bool => 1,
            Type::Char => 4,
            _ => 8, // pointer-sized
        }
    }
}

/// A type definition stored in the type environment.
#[derive(Debug, Clone)]
pub enum TypeInfo {
    Def {
        name: String,
        generics: Vec<String>,
        fields: Vec<(String, Type)>,
    },
    Enum {
        name: String,
        generics: Vec<String>,
        variants: Vec<(String, Option<Type>)>,
    },
    Spec {
        name: String,
        fields: Vec<(String, Type)>,
        methods: Vec<(String, Vec<Type>, Option<Type>)>,
    },
    Alias {
        name: String,
        target: Type,
    },
}

/// A function signature stored in the environment.
#[derive(Debug, Clone)]
pub struct FnInfo {
    pub name: String,
    pub params: Vec<(String, Type)>,
    pub return_type: Type,
    pub is_pub: bool,
    pub receiver_type: Option<Type>, // For methods
}

/// The type environment / symbol table.
#[derive(Debug, Clone)]
pub struct TypeEnv {
    /// Named types.
    pub types: HashMap<String, TypeInfo>,
    /// Function signatures.
    pub functions: HashMap<String, Vec<FnInfo>>,
    /// Variable bindings in scope.
    pub vars: HashMap<String, Type>,
    /// Parent scope (for nested scopes).
    pub parent: Option<Box<TypeEnv>>,
}

impl Default for TypeEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeEnv {
    pub fn new() -> Self {
        let mut env = TypeEnv {
            types: HashMap::new(),
            functions: HashMap::new(),
            vars: HashMap::new(),
            parent: None,
        };
        env.register_builtins();
        env
    }

    pub fn child(&self) -> Self {
        TypeEnv {
            types: self.types.clone(),
            functions: self.functions.clone(),
            vars: HashMap::new(),
            parent: Some(Box::new(self.clone())),
        }
    }

    pub fn lookup_var(&self, name: &str) -> Option<Type> {
        self.vars
            .get(name)
            .cloned()
            .or_else(|| self.parent.as_ref().and_then(|p| p.lookup_var(name)))
    }

    pub fn lookup_type(&self, name: &str) -> Option<TypeInfo> {
        self.types.get(name).cloned()
    }

    pub fn lookup_fn(&self, name: &str) -> Option<&Vec<FnInfo>> {
        self.functions.get(name)
    }

    pub fn define_var(&mut self, name: String, ty: Type) {
        self.vars.insert(name, ty);
    }

    pub fn define_type(&mut self, name: String, info: TypeInfo) {
        self.types.insert(name, info);
    }

    pub fn define_fn(&mut self, name: String, info: FnInfo) {
        self.functions.entry(name).or_default().push(info);
    }

    fn register_builtins(&mut self) {
        // Register print and other builtins
        self.define_fn(
            "print".to_string(),
            FnInfo {
                name: "print".to_string(),
                params: vec![("msg".to_string(), Type::Str)],
                return_type: Type::Unit,
                is_pub: true,
                receiver_type: None,
            },
        );

        // Register Option type
        self.define_type(
            "Option".to_string(),
            TypeInfo::Enum {
                name: "Option".to_string(),
                generics: vec!["T".to_string()],
                variants: vec![
                    ("Some".to_string(), Some(Type::Var("T".to_string()))),
                    ("None".to_string(), None),
                ],
            },
        );

        // Register Result type
        self.define_type(
            "Result".to_string(),
            TypeInfo::Enum {
                name: "Result".to_string(),
                generics: vec!["T".to_string(), "E".to_string()],
                variants: vec![
                    ("Ok".to_string(), Some(Type::Var("T".to_string()))),
                    ("Err".to_string(), Some(Type::Var("E".to_string()))),
                ],
            },
        );
    }
}
