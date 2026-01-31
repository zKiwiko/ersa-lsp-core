use once_cell::sync::Lazy;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct BuiltinFunction {
    pub name: String,
    pub parameters: Vec<String>,
    pub description: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Snippet {
    pub name: String,
    pub snippet: String,
    pub description: String,
}

pub const DATATYPES: &[&str] = &[
    "int", "uint8", "uint16", "int8", "int16", "char", "ps5adt", "byte", "string",
];

pub const KEYWORDS: &[&str] = &[
    "main", "init", "combo", "fcombo", "function", "if", "else", "while", "for", "return", "break",
    "continue", "const", "enum", "define", "do", "switch", "case", "default",
];

static CONSTANTS: Lazy<Vec<String>> = Lazy::new(|| {
    const CONSTANTS_JSON: &str = include_str!("../../data/constants.json");
    serde_json::from_str(CONSTANTS_JSON).expect("Failed to parse constants.json")
});

pub fn get_constants() -> &'static [String] {
    &CONSTANTS
}

static BUILTIN_FUNCTIONS: Lazy<Vec<BuiltinFunction>> = Lazy::new(|| {
    const FUNCTIONS_JSON: &str = include_str!("../../data/functions.json");
    serde_json::from_str(FUNCTIONS_JSON).expect("Failed to parse functions.json")
});

pub fn get_builtins() -> &'static [BuiltinFunction] {
    &BUILTIN_FUNCTIONS
}

static SNIPPETS: Lazy<Vec<Snippet>> = Lazy::new(|| {
    const SNIPPETS_JSON: &str = include_str!("../../data/snippets.json");
    serde_json::from_str(SNIPPETS_JSON).expect("Failed to parse snippets.json")
});

pub fn get_snippets() -> &'static [Snippet] {
    &SNIPPETS
}
