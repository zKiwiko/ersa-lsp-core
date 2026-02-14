#[derive(Debug, Clone)]
pub struct Location {
    pub uri: String,
    pub range: tower_lsp::lsp_types::Range,
}

#[derive(Debug, Clone)]
pub struct UserFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub definition: Location,
    pub body_range: Option<tower_lsp::lsp_types::Range>,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UserMacro {
    pub name: String,
    pub parameters: Vec<String>,
    pub definition: Location,
    pub body_range: Option<tower_lsp::lsp_types::Range>,
    pub documentation: Option<String>,
    pub has_placeholder: bool,
}

#[derive(Debug, Clone)]
pub enum DataTypes {
    Int8,
    Int16,
    Int32,
    Uint8,
    Uint16,
    Byte,
    Char,
    String,
    Image,
    Ps5adt,
}
#[derive(Debug, Clone)]
pub enum Mutability {
    Mutable,
    Immutable,
}

#[derive(Debug, Clone)]
pub struct VarType {
    pub mutability: Mutability,
    /// 0 = not an array, 1 = 1D array, 2 = 2D array, etc.
    pub array_dims: u8,
}

#[derive(Debug, Clone)]
pub enum VariableKind {
    Regular,
    Define,
    EnumMember,
}

#[derive(Debug, Clone)]
pub struct UserVariable {
    pub name: String,
    pub data_type: Option<DataTypes>,
    pub var_type: Option<VarType>,
    pub kind: VariableKind,
    pub definition: Location,
    pub documentation: Option<String>,
}
