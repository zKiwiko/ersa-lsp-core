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

        let var_nodes = find_by_kind(&mut cursor, "variable_declaration");
        for node in var_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }
            variables.extend(Self::extract_variables_from_declaration(
                node,
                source,
                uri,
                types::Mutability::Mutable,
            ));
        }

        cursor = root.walk();
        let const_nodes = find_by_kind(&mut cursor, "const_variable_declaration");
        for node in const_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }
            variables.extend(Self::extract_variables_from_declaration(
                node,
                source,
                uri,
                types::Mutability::Immutable,
            ));
        }

        cursor = root.walk();
        let define_nodes = find_by_kind(&mut cursor, "define_declaration");
        for node in define_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }
            if let Some(var) = Self::extract_define_variable(node, source, uri) {
                variables.push(var);
            }
        }

        cursor = root.walk();
        let enum_nodes = find_by_kind(&mut cursor, "enum_declaration");
        for node in enum_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }
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

        let data_type = get_child_by_kind(node, "type")
            .and_then(|type_node| type_node.utf8_text(source.as_bytes()).ok())
            .and_then(Self::parse_data_type);

        let declarators = get_children_by_kind(node, "variable_declarator");

        for declarator in declarators {
            let Some(name) = get_child_by_kind(declarator, "identifier")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|s| s.to_string())
            else {
                continue;
            };

            let array_dims = get_children_by_kind(declarator, "array_dimension").len() as u8;

            let var_type = Some(types::VarType {
                mutability: mutability.clone(),
                array_dims,
            });

            let documentation = Self::extract_documentation_before_node(declarator, source);

            variables.push(types::UserVariable {
                name,
                data_type: data_type.clone(),
                var_type,
                kind: types::VariableKind::Regular,
                definition: Self::node_to_location(declarator, uri),
                documentation,
            });
        }

        variables
    }

    fn extract_define_variable(node: Node, source: &str, uri: &str) -> Option<types::UserVariable> {
        let ident_node = get_child_by_kind(node, "identifier")?;
        let name = ident_node
            .utf8_text(source.as_bytes())
            .ok()?
            .to_string();

        let documentation = Self::extract_documentation_before_node(node, source);

        Some(types::UserVariable {
            name,
            data_type: types::DataTypes::Int32.into(),
            var_type: Some(types::VarType {
                mutability: types::Mutability::Immutable,
                array_dims: 0,
            }),
            kind: types::VariableKind::Define,
            definition: Self::node_to_location(ident_node, uri),
            documentation,
        })
    }

    fn extract_enum_members(node: Node, source: &str, uri: &str) -> Vec<types::UserVariable> {
        let mut members = Vec::new();

        let Some(variant_list) = get_child_by_kind(node, "enum_variant_list") else {
            return members;
        };

        let variants = get_children_by_kind(variant_list, "enum_variant");

        for variant in variants {
            let Some(name) = get_child_by_kind(variant, "identifier")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|s| s.to_string())
            else {
                continue;
            };

            let documentation = Self::extract_documentation_before_node(variant, source);

            members.push(types::UserVariable {
                name,
                data_type: Some(types::DataTypes::Int32),
                var_type: Some(types::VarType {
                    mutability: types::Mutability::Immutable,
                    array_dims: 0,
                }),
                kind: types::VariableKind::EnumMember,
                definition: Self::node_to_location(variant, uri),
                documentation,
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

    pub fn extract_user_functions(&mut self, source: &str, uri: &str) -> Vec<types::UserFunction> {
        let Some(tree) = self.parse(source) else {
            return Vec::new();
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let function_nodes = find_by_kind(&mut cursor, "function_declaration");

        function_nodes
            .into_iter()
            .filter(|node| !node.is_error() && !node.is_missing() && !node.has_error())
            .filter_map(|node| Self::extract_function_info(node, source, uri))
            .collect()
    }

    pub fn extract_user_macros(&mut self, source: &str, uri: &str) -> Vec<types::UserMacro> {
        let Some(tree) = self.parse(source) else {
            return Vec::new();
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let macro_nodes = find_by_kind(&mut cursor, "macro_declaration");

        macro_nodes
            .into_iter()
            .filter(|node| !node.is_error() && !node.is_missing() && !node.has_error())
            .filter_map(|node| Self::extract_macro_info(node, source, uri))
            .collect()
    }

    fn extract_function_info(node: Node, source: &str, uri: &str) -> Option<types::UserFunction> {
        let name = get_child_by_kind(node, "identifier")?
            .utf8_text(source.as_bytes())
            .ok()?
            .to_string();

        let parameters = get_child_by_kind(node, "parameter_list")
            .map(|param_list| {
                get_children_by_kind(param_list, "identifier")
                    .into_iter()
                    .filter_map(|param| param.utf8_text(source.as_bytes()).ok())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let body_range = get_child_by_kind(node, "block").map(|body_node| {
            let start = body_node.start_position();
            let end = body_node.end_position();
            tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: start.row as u32,
                    character: start.column as u32,
                },
                end: tower_lsp::lsp_types::Position {
                    line: end.row as u32,
                    character: end.column as u32,
                },
            }
        });

        let documentation = Self::extract_documentation_before_node(node, source);

        Some(types::UserFunction {
            name,
            parameters,
            definition: Self::node_to_location(node, uri),
            body_range,
            documentation,
        })
    }

    fn extract_macro_info(node: Node, source: &str, uri: &str) -> Option<types::UserMacro> {
        let name = get_child_by_kind(node, "identifier")?
            .utf8_text(source.as_bytes())
            .ok()?
            .to_string();

        let parameters = get_child_by_kind(node, "parameter_list")
            .map(|param_list| {
                get_children_by_kind(param_list, "identifier")
                    .into_iter()
                    .filter_map(|param| param.utf8_text(source.as_bytes()).ok())
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let body_range = get_child_by_kind(node, "block").map(|body_node| {
            let start = body_node.start_position();
            let end = body_node.end_position();
            tower_lsp::lsp_types::Range {
                start: tower_lsp::lsp_types::Position {
                    line: start.row as u32,
                    character: start.column as u32,
                },
                end: tower_lsp::lsp_types::Position {
                    line: end.row as u32,
                    character: end.column as u32,
                },
            }
        });

        let has_placeholder = get_child_by_kind(node, "block")
            .map(|body_node| {
                body_node
                    .utf8_text(source.as_bytes())
                    .ok()
                    .map(|text| text.contains("%0"))
                    .unwrap_or(false)
            })
            .unwrap_or(false);

        let documentation = Self::extract_documentation_before_node(node, source);

        Some(types::UserMacro {
            name,
            parameters,
            definition: Self::node_to_location(node, uri),
            body_range,
            documentation,
            has_placeholder,
        })
    }

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

    fn extract_documentation_before_node(node: Node, source: &str) -> Option<String> {
        let start_line = node.start_position().row;
        if start_line == 0 {
            return None;
        }

        let lines: Vec<&str> = source.lines().collect();
        let mut doc_lines = Vec::new();
        let mut current_line = start_line.saturating_sub(1);

        loop {
            if current_line >= lines.len() {
                break;
            }

            let line = lines[current_line].trim();

            if line.starts_with("//") {
                let comment = line.trim_start_matches("//").trim();
                doc_lines.push(comment.to_string());
            } else if line.starts_with("/*") || line.contains("*/") {
                let comment = line
                    .trim_start_matches("/*")
                    .trim_end_matches("*/")
                    .trim_start_matches('*')
                    .trim();
                if !comment.is_empty() {
                    doc_lines.push(comment.to_string());
                }
            } else if !line.is_empty() {
                break;
            }

            if current_line == 0 {
                break;
            }
            current_line = current_line.saturating_sub(1);
        }

        if doc_lines.is_empty() {
            None
        } else {
            doc_lines.reverse();
            Some(doc_lines.join("\n"))
        }
    }

    pub fn find_assignments(&mut self, source: &str) -> Vec<(String, usize, usize)> {
        let mut assignments = Vec::new();

        let Some(tree) = self.parse(source) else {
            return assignments;
        };

        let root = tree.root_node();
        let mut cursor = root.walk();

        let assignment_nodes = find_by_kind(&mut cursor, "assignment_statement");

        for node in assignment_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }

            let Some(left) = node.child(0) else {
                continue;
            };

            if left.kind() != "expression" {
                continue;
            }

            let var_name = self.extract_assignment_target(left, source);

            if let Some(name) = var_name {
                assignments.push((
                    name,
                    left.start_position().row,
                    left.start_position().column,
                ));
            }
        }

        cursor = root.walk();
        let assignment_no_semi_nodes = find_by_kind(&mut cursor, "assignment_statement_no_semi");

        for node in assignment_no_semi_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }

            let Some(left) = node.child(0) else {
                continue;
            };

            if left.kind() != "expression" {
                continue;
            }

            let var_name = self.extract_assignment_target(left, source);

            if let Some(name) = var_name {
                assignments.push((
                    name,
                    left.start_position().row,
                    left.start_position().column,
                ));
            }
        }

        assignments
    }

    fn extract_assignment_target(&self, node: Node, source: &str) -> Option<String> {
        let target = if node.kind() == "expression" {
            node.child(0)?
        } else {
            node
        };

        match target.kind() {
            "identifier" => target
                .utf8_text(source.as_bytes())
                .ok()
                .map(|s| s.to_string()),
            "array_access" => {
                let mut current = target;
                while current.kind() == "array_access" {
                    if let Some(object) = current.child(0) {
                        current = if object.kind() == "expression" {
                            object.child(0).unwrap_or(object)
                        } else {
                            object
                        };
                    } else {
                        break;
                    }
                }

                if current.kind() == "identifier" {
                    current
                        .utf8_text(source.as_bytes())
                        .ok()
                        .map(|s| s.to_string())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn find_function_calls(&mut self, source: &str) -> Vec<(String, usize, usize, usize)> {
        let mut calls = Vec::new();

        let Some(tree) = self.parse(source) else {
            return calls;
        };

        let root = tree.root_node();
        let mut cursor = root.walk();

        let call_expr_nodes = find_by_kind(&mut cursor, "call_expression");
        cursor = root.walk();
        let call_stmt_nodes = find_by_kind(&mut cursor, "call_statement");

        cursor = root.walk();
        let macro_call_positions: std::collections::HashSet<(usize, usize)> =
            find_by_kind(&mut cursor, "macro_call")
                .iter()
                .filter_map(|node| {
                    node.child(0)
                        .map(|child| (child.start_position().row, child.start_position().column))
                })
                .collect();

        for node in call_expr_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }

            if let Some(expr_node) = node.child(0) {
                let func_name_node = if expr_node.kind() == "expression" {
                    expr_node.child(0)
                } else if expr_node.kind() == "identifier" {
                    Some(expr_node)
                } else {
                    None
                };

                if let Some(func_node) = func_name_node {
                    if func_node.kind() == "identifier" {
                        if let Ok(func_name) = func_node.utf8_text(source.as_bytes()) {
                            let pos = (
                                func_node.start_position().row,
                                func_node.start_position().column,
                            );

                            if macro_call_positions.contains(&pos) {
                                continue;
                            }

                            let arg_count = if let Some(arg_list_node) =
                                get_child_by_kind(node, "argument_list")
                            {
                                get_children_by_kind(arg_list_node, "expression").len()
                            } else {
                                0
                            };

                            calls.push((
                                func_name.to_string(),
                                func_node.start_position().row,
                                func_node.start_position().column,
                                arg_count,
                            ));
                        }
                    }
                }
            }
        }

        for node in call_stmt_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }

            if let Some(func_node) = get_child_by_kind(node, "identifier") {
                if let Ok(func_name) = func_node.utf8_text(source.as_bytes()) {
                    let arg_count =
                        if let Some(arg_list_node) = get_child_by_kind(node, "argument_list") {
                            get_children_by_kind(arg_list_node, "expression").len()
                        } else {
                            0
                        };

                    calls.push((
                        func_name.to_string(),
                        func_node.start_position().row,
                        func_node.start_position().column,
                        arg_count,
                    ));
                }
            }
        }

        calls
    }

    pub fn find_variable_references(&mut self, source: &str) -> Vec<(String, usize, usize)> {
        let mut refs = Vec::new();

        let Some(tree) = self.parse(source) else {
            return refs;
        };

        let root = tree.root_node();

        self.collect_variable_refs(root, source, &mut refs);

        refs
    }

    fn collect_variable_refs(
        &self,
        node: Node,
        source: &str,
        refs: &mut Vec<(String, usize, usize)>,
    ) {
        match node.kind() {
            "variable_declarator"
            | "define_declaration"
            | "enum_variant"
            | "function_declaration"
            | "macro_declaration" => {
                return;
            }
            _ => {}
        }

        if node.kind() == "identifier" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "call_statement" {
                    if parent.child(0).map(|c| c.id()) == Some(node.id()) {
                        return;
                    }
                }

                if parent.kind() == "macro_call" {
                    if parent.child(0).map(|c| c.id()) == Some(node.id()) {
                        return;
                    }
                }

                if parent.kind() == "expression" {
                    if let Some(grandparent) = parent.parent() {
                        if grandparent.kind() == "call_expression" {
                            if grandparent.child(0).map(|c| c.id()) == Some(parent.id()) {
                                return;
                            }
                        }
                    }
                }

                if parent.kind() == "variable_declarator" {
                    if let Some(id_node) = get_child_by_kind(parent, "identifier") {
                        if id_node.id() == node.id() {
                            return;
                        }
                    }
                }
            }

            if let Ok(name) = node.utf8_text(source.as_bytes()) {
                refs.push((
                    name.to_string(),
                    node.start_position().row,
                    node.start_position().column,
                ));
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_variable_refs(child, source, refs);
        }
    }

    pub fn extract_imports(&mut self, source: &str) -> Vec<String> {
        let mut imports = Vec::new();

        let Some(tree) = self.parse(source) else {
            return imports;
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let import_nodes = find_by_kind(&mut cursor, "import");

        for node in import_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }

            if let Some(path_node) = get_child_by_kind(node, "import_identifier") {
                if let Ok(path) = path_node.utf8_text(source.as_bytes()) {
                    let path_str = path.trim_matches('"');

                    let full_path = if path_str.ends_with(".gpc") {
                        path_str.to_string()
                    } else {
                        format!("{}.gpc", path_str)
                    };
                    imports.push(full_path);
                }
            }
        }

        imports
    }

    pub fn find_macro_calls(&mut self, source: &str) -> Vec<(String, usize, usize, bool)> {
        let mut calls = Vec::new();

        let Some(tree) = self.parse(source) else {
            return calls;
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let macro_call_nodes = find_by_kind(&mut cursor, "macro_call");

        for node in macro_call_nodes {
            if node.is_error() || node.is_missing() || node.has_error() {
                continue;
            }

            if let Some(name_node) = node.child(0) {
                if name_node.kind() == "identifier" {
                    if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                        let has_body = node
                            .children(&mut node.walk())
                            .any(|child| child.kind() == "{");

                        calls.push((
                            name.to_string(),
                            name_node.start_position().row,
                            name_node.start_position().column,
                            has_body,
                        ));
                    }
                }
            }
        }

        calls
    }
}
