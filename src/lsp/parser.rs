use tree_sitter::{Language, Node, Parser, Tree};

use super::tree_visitor::{find_by_kind, get_child_by_kind, get_children_by_kind};

#[allow(dead_code)]
extern "C" {
    fn tree_sitter_gpc() -> Language;
}

#[derive(Debug, Clone)]
pub struct UserFunction {
    pub name: String,
    pub parameters: Vec<String>,
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

    /// Extract all user-defined functions from the source code
    pub fn extract_user_functions(&mut self, source: &str) -> Vec<UserFunction> {
        let Some(tree) = self.parse(source) else {
            return Vec::new();
        };

        let root = tree.root_node();
        let mut cursor = root.walk();
        let function_nodes = find_by_kind(&mut cursor, "function_declaration");

        function_nodes
            .into_iter()
            .filter_map(|node| Self::extract_function_info(node, source))
            .collect()
    }

    fn extract_function_info(node: Node, source: &str) -> Option<UserFunction> {
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

        Some(UserFunction { name, parameters })
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
            if node.is_error() || node.is_missing() {
                let start = node.start_position();
                let message = if node.is_missing() {
                    format!("Missing {}", node.kind())
                } else {
                    "Syntax error".to_string()
                };
                errors.push((start.row, start.column, message));
            }
        });

        errors
    }
}
