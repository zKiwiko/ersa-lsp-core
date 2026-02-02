pub mod types;

use tree_sitter::{Language, Node, Parser, Tree};

use super::tree_visitor::{find_by_kind, get_child_by_kind, get_children_by_kind};

#[allow(dead_code)]
extern "C" {
    fn tree_sitter_gpc() -> Language;
}

pub struct GpcParser {
    parser: Parser,
}

impl GpcParser {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        let language = unsafe { tree_sitter_gpc() };
        parser
            .set_language(&language)
            .expect("Error loading GPC language");
        GpcParser { parser }
    }

    pub fn parse(&mut self, source: &str) -> Option<Tree> {
        self.parser.parse(source, None)
    }

    pub fn extract_user_variables(&mut self, source: &str, uri: &str) -> Vec<types::UserVariable> {
        let Some(tree) = self.parse(source) else {
            return Vec::new();
        };

        let root = tree.root_node();
        let mut cursor = root.walk();

        let mut variables = Vec::new();

        // Collect mutable variables
        let var_nodes = find_by_kind(&mut cursor, "variable_declaration");
        for node in var_nodes {
            variables.extend(Self::extract_variables_from_declaration(
                node,
                source,
                uri,
                types::Mutability::Mutable,
            ));
        }

        // Collect const variables
        cursor = root.walk();
        let const_nodes = find_by_kind(&mut cursor, "const_variable_declaration");
        for node in const_nodes {
            variables.extend(Self::extract_variables_from_declaration(
                node,
                source,
                uri,
                types::Mutability::Immutable,
            ));
        }

        // Collect define variables
        cursor = root.walk();
        let define_nodes = find_by_kind(&mut cursor, "define_declaration");
        for node in define_nodes {
            if let Some(var) = Self::extract_define_variable(node, source, uri) {
                variables.push(var);
            }
        }

        // Collect enum members
        cursor = root.walk();
        let enum_nodes = find_by_kind(&mut cursor, "enum_declaration");
        for node in enum_nodes {
            variables.extend(Self::extract_enum_members(node, source, uri));
        }

        variables
    }

    fn extract_variables_from_declaration(
        node: Node,
        source: &str,
        uri: &str,
        mutability: types::Mutability,
    ) -> Vec<types::UserVariable> {
        let mut variables = Vec::new();

        // Get the type field
        let data_type = get_child_by_kind(node, "type")
            .and_then(|type_node| type_node.utf8_text(source.as_bytes()).ok())
            .and_then(Self::parse_data_type);

        // Get all variable_declarator children
        let declarators = get_children_by_kind(node, "variable_declarator");

        for declarator in declarators {
            // Get the identifier (name)
            let Some(name) = get_child_by_kind(declarator, "identifier")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|s| s.to_string())
            else {
                continue;
            };

            // Count array dimensions
            let array_dims = get_children_by_kind(declarator, "array_dimension").len() as u8;

            let var_type = Some(types::VarType {
                mutability: mutability.clone(),
                array_dims,
            });

            variables.push(types::UserVariable {
                name,
                data_type: data_type.clone(),
                var_type,
                kind: types::VariableKind::Regular,
                definition: Self::node_to_location(declarator, uri),
            });
        }

        variables
    }

    fn extract_define_variable(node: Node, source: &str, uri: &str) -> Option<types::UserVariable> {
        let name = get_child_by_kind(node, "identifier")?
            .utf8_text(source.as_bytes())
            .ok()?
            .to_string();

        Some(types::UserVariable {
            name,
            data_type: types::DataTypes::Int32.into(),
            var_type: Some(types::VarType {
                mutability: types::Mutability::Immutable,
                array_dims: 0,
            }),
            kind: types::VariableKind::Regular,
            definition: Self::node_to_location(node, uri),
        })
    }

    fn extract_enum_members(node: Node, source: &str, uri: &str) -> Vec<types::UserVariable> {
        let mut members = Vec::new();

        // Get the enum_variant_list
        let Some(variant_list) = get_child_by_kind(node, "enum_variant_list") else {
            return members;
        };

        // Get all enum_variant children
        let variants = get_children_by_kind(variant_list, "enum_variant");

        for variant in variants {
            // Get the identifier (name)
            let Some(name) = get_child_by_kind(variant, "identifier")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|s| s.to_string())
            else {
                continue;
            };

            members.push(types::UserVariable {
                name,
                data_type: Some(types::DataTypes::Int32),
                var_type: Some(types::VarType {
                    mutability: types::Mutability::Immutable,
                    array_dims: 0,
                }),
                kind: types::VariableKind::EnumMember,
                definition: Self::node_to_location(variant, uri),
            });
        }

        members
    }

    fn parse_data_type(type_str: &str) -> Option<types::DataTypes> {
        match type_str {
            "int" => Some(types::DataTypes::Int32),
            "int8" => Some(types::DataTypes::Int8),
            "int16" => Some(types::DataTypes::Int16),
            "uint8" => Some(types::DataTypes::Uint8),
            "uint16" => Some(types::DataTypes::Uint16),
            "byte" => Some(types::DataTypes::Byte),
            "char" => Some(types::DataTypes::Char),
            "string" => Some(types::DataTypes::String),
            "image" => Some(types::DataTypes::Image),
            "ps5adt" => Some(types::DataTypes::Ps5adt),
            _ => None,
        }
    }

    /// Extract all user-defined functions from the source code
    pub fn extract_user_functions(&mut self, source: &str, uri: &str) -> Vec<types::UserFunction> {
        let Some(tree) = self.parse(source) else {
            return Vec::new();
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let function_nodes = find_by_kind(&mut cursor, "function_declaration");

        function_nodes
            .into_iter()
            .filter_map(|node| Self::extract_function_info(node, source, uri))
            .collect()
    }

    fn extract_function_info(node: Node, source: &str, uri: &str) -> Option<types::UserFunction> {
        // Get the function name (first identifier child)
        let name = get_child_by_kind(node, "identifier")?
            .utf8_text(source.as_bytes())
            .ok()?
            .to_string();

        // Get parameters from parameter_list
        let parameters = get_child_by_kind(node, "parameter_list")
            .map(|param_list| {
                get_children_by_kind(param_list, "identifier")
                    .into_iter()
                    .filter_map(|param| param.utf8_text(source.as_bytes()).ok())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        Some(types::UserFunction { 
            name, 
            parameters,
            definition: Self::node_to_location(node, uri),
        })
    }

    /// Find syntax errors in the parsed tree
    pub fn find_syntax_errors(&mut self, source: &str) -> Vec<(usize, usize, String)> {
        let Some(tree) = self.parse(source) else {
            return Vec::new();
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let mut errors = Vec::new();

        super::tree_visitor::visit_tree(&mut cursor, &mut |node: Node| {
            if node.is_error() {
                let start = node.start_position();
                let message = if let Ok(text) = node.utf8_text(source.as_bytes()) {
                    format!("Syntax error: unexpected '{}'", text.trim())
                } else {
                    "Syntax error".to_string()
                };
                errors.push((start.row, start.column, message));
            } else if node.is_missing() {
                let position = if let Some(parent) = node.parent() {
                    parent.start_position()
                } else {
                    node.start_position()
                };

                let message = format!("Missing '{}'.", node.kind());
                errors.push((position.row, position.column, message));
            }
        });

        errors
    }

    /// Helper function to create a Location from a tree-sitter Node
    fn node_to_location(node: Node, uri: &str) -> types::Location {
        types::Location {
            uri: uri.to_string(),
            range: tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: node.start_position().row as u32,
                    character: node.start_position().column as u32,
                },
                end: tower_lsp::lsp_types::Position {
                    line: node.end_position().row as u32,
                    character: node.end_position().column as u32,
                },
            },
        }
    }
}
