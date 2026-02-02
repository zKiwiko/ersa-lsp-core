pub mod data;
pub mod handlers;
pub mod parser;
mod tree_visitor;
pub mod utils;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

pub struct LSP {
    client: Client,
    documents: Arc<Mutex<HashMap<String, String>>>,
    parser: Arc<Mutex<parser::GpcParser>>,
    user_functions: Arc<Mutex<HashMap<String, Vec<parser::types::UserFunction>>>>,
    user_variables: Arc<Mutex<HashMap<String, Vec<parser::types::UserVariable>>>>,
    last_edit_times: Arc<Mutex<HashMap<String, SystemTime>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for LSP {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        self.handle_initialize(params).await
    }

    async fn initialized(&self, params: InitializedParams) {
        self.handle_initialized(params).await;
    }

    async fn shutdown(&self) -> Result<()> {
        self.handle_shutdown().await
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.handle_did_open(params).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.handle_did_change(params).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        self.handle_did_save(params).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.handle_did_close(params).await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.handle_completion(params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.handle_hover(params).await
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        self.handle_inlay_hint(params).await
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        self.handle_goto_definition(params).await
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        self.handle_references(params).await
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        self.handle_signature_help(params).await
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        self.handle_document_symbol(params).await
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        self.handle_document_highlight(params).await
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        self.handle_rename(params).await
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        self.handle_folding_range(params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        self.handle_semantic_tokens_full(params).await
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        self.handle_code_lens(params).await
    }
}

impl LSP {
    pub fn new(client: Client) -> Self {
        LSP {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
            parser: Arc::new(Mutex::new(parser::GpcParser::new())),
            user_functions: Arc::new(Mutex::new(HashMap::new())),
            user_variables: Arc::new(Mutex::new(HashMap::new())),
            last_edit_times: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start() {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(|client| LSP::new(client));
        Server::new(stdin, stdout, socket).serve(service).await;
    }

    pub async fn log(&self, message: &str) {
        self.client
            .log_message(MessageType::INFO, message.to_string())
            .await;
    }

    pub async fn update_user_functions(&self, uri: &str, text: &str) {
        let functions = {
            let mut parser = self.parser.lock().unwrap();
            parser.extract_user_functions(text, uri)
        };

        self.user_functions
            .lock()
            .unwrap()
            .insert(uri.to_string(), functions);
    }

    pub async fn update_user_variables(&self, uri: &str, text: &str) {
        let variables = {
            let mut parser = self.parser.lock().unwrap();
            parser.extract_user_variables(text, uri)
        };

        self.user_variables
            .lock()
            .unwrap()
            .insert(uri.to_string(), variables);
    }

    fn check_duplicate_definitions(&self, uri: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(uri) {
            let mut sorted_funcs: Vec<_> = funcs.iter().collect();
            sorted_funcs.sort_by_key(|f| {
                (
                    f.definition.range.start.line,
                    f.definition.range.start.character,
                )
            });

            let mut seen_names: HashMap<String, &parser::types::UserFunction> = HashMap::new();

            for func in sorted_funcs {
                if let Some(first_def) = seen_names.get(&func.name) {
                    diagnostics.push(Diagnostic {
                        range: func.definition.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("duplicate-function".to_string())),
                        code_description: None,
                        source: Some("ersa_lsp".to_string()),
                        message: format!(
                            "Function '{}' is already defined on line {}",
                            func.name,
                            first_def.definition.range.start.line + 1
                        ),
                        related_information: Some(vec![DiagnosticRelatedInformation {
                            location: tower_lsp::lsp_types::Location {
                                uri: Url::parse(&first_def.definition.uri).unwrap(),
                                range: first_def.definition.range,
                            },
                            message: "First defined here".to_string(),
                        }]),
                        tags: None,
                        data: None,
                    });
                } else {
                    seen_names.insert(func.name.clone(), func);
                }
            }
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(uri) {
            let mut sorted_vars: Vec<_> = vars.iter().collect();
            sorted_vars.sort_by_key(|v| {
                (
                    v.definition.range.start.line,
                    v.definition.range.start.character,
                )
            });

            let mut seen_names: HashMap<String, &parser::types::UserVariable> = HashMap::new();

            for var in sorted_vars {
                if let Some(first_def) = seen_names.get(&var.name) {
                    diagnostics.push(Diagnostic {
                        range: var.definition.range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("duplicate-variable".to_string())),
                        code_description: None,
                        source: Some("ersa_lsp".to_string()),
                        message: format!(
                            "Variable '{}' is already defined on line {}",
                            var.name,
                            first_def.definition.range.start.line + 1
                        ),
                        related_information: Some(vec![DiagnosticRelatedInformation {
                            location: tower_lsp::lsp_types::Location {
                                uri: Url::parse(&first_def.definition.uri).unwrap(),
                                range: first_def.definition.range,
                            },
                            message: "First defined here".to_string(),
                        }]),
                        tags: None,
                        data: None,
                    });
                } else {
                    seen_names.insert(var.name.clone(), var);
                }
            }
        }

        diagnostics
    }

    pub async fn publish_diagnostics(&self, uri: &str, text: &str) {
        let mut diagnostics = Vec::new();

        let errors = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_syntax_errors(text)
        };

        diagnostics.extend(errors.into_iter().map(|(line, col, message)| Diagnostic {
            range: Range {
                start: Position {
                    line: line as u32,
                    character: col as u32,
                },
                end: Position {
                    line: line as u32,
                    character: (col + 1) as u32,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: Some(NumberOrString::String("syntax-error".to_string())),
            code_description: None,
            source: Some("ersa_lsp".to_string()),
            message,
            related_information: None,
            tags: None,
            data: None,
        }));

        diagnostics.extend(self.check_duplicate_definitions(uri));

        diagnostics.extend(self.check_enum_case(uri));

        diagnostics.extend(self.check_unused_symbols(uri, text));

        diagnostics.extend(self.check_immutable_assignments(uri, text));

        diagnostics.extend(self.check_undefined_variables(uri, text));

        diagnostics.extend(self.check_undefined_functions(uri, text));

        self.client
            .publish_diagnostics(Url::parse(uri).unwrap(), diagnostics, None)
            .await;
    }

    fn check_enum_case(&self, uri: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(uri) {
            for var in vars {
                if !is_upper_snake_case(&var.name) && might_be_constant(&var.name) {
                    diagnostics.push(Diagnostic {
                        range: var.definition.range,
                        severity: Some(DiagnosticSeverity::HINT),
                        code: Some(NumberOrString::String("enum-case".to_string())),
                        code_description: None,
                        source: Some("ersa_lsp".to_string()),
                        message: format!(
                            "Constant '{}' should use UPPER_SNAKE_CASE naming convention",
                            var.name
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            }
        }

        diagnostics
    }

    fn check_unused_symbols(&self, uri: &str, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = text.lines().collect();

        let find_word_positions = |lines: &[&str], word: &str| -> Vec<Range> {
            let mut positions = Vec::new();
            for (line_idx, line_text) in lines.iter().enumerate() {
                let mut start = 0;
                while let Some(pos) = line_text[start..].find(word) {
                    let actual_pos = start + pos;

                    let is_start = actual_pos == 0
                        || !line_text
                            .chars()
                            .nth(actual_pos - 1)
                            .map(|c| c.is_alphanumeric() || c == '_')
                            .unwrap_or(false);

                    let is_end = actual_pos + word.len() >= line_text.len()
                        || !line_text
                            .chars()
                            .nth(actual_pos + word.len())
                            .map(|c| c.is_alphanumeric() || c == '_')
                            .unwrap_or(false);

                    if is_start && is_end {
                        positions.push(Range {
                            start: Position {
                                line: line_idx as u32,
                                character: actual_pos as u32,
                            },
                            end: Position {
                                line: line_idx as u32,
                                character: (actual_pos + word.len()) as u32,
                            },
                        });
                    }

                    start = actual_pos + 1;
                }
            }
            positions
        };

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(uri) {
            for func in funcs {
                let all_positions = find_word_positions(&lines, &func.name);
                let ref_count = all_positions
                    .iter()
                    .filter(|r| {
                        let pos = r.start;
                        let range = &func.definition.range;

                        if pos.line < range.start.line || pos.line > range.end.line {
                            return true;
                        }

                        if pos.line == range.start.line && pos.character < range.start.character {
                            return true;
                        }

                        if pos.line == range.end.line && pos.character >= range.end.character {
                            return true;
                        }

                        false
                    })
                    .count();

                if ref_count == 0 {
                    diagnostics.push(Diagnostic {
                        range: func.definition.range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("unused-function".to_string())),
                        code_description: None,
                        source: Some("ersa_lsp".to_string()),
                        message: format!("Function '{}' is never used", func.name),
                        related_information: None,
                        tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                        data: None,
                    });
                }
            }
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(uri) {
            for var in vars {
                let all_positions = find_word_positions(&lines, &var.name);
                let ref_count = all_positions
                    .iter()
                    .filter(|r| {
                        let pos = r.start;
                        let range = &var.definition.range;

                        if pos.line < range.start.line || pos.line > range.end.line {
                            return true;
                        }

                        if pos.line == range.start.line && pos.character < range.start.character {
                            return true;
                        }

                        if pos.line == range.end.line && pos.character >= range.end.character {
                            return true;
                        }

                        false
                    })
                    .count();

                if ref_count == 0 {
                    let symbol_type = match var.kind {
                        parser::types::VariableKind::EnumMember => "Enum member",
                        parser::types::VariableKind::Define => "Define",
                        parser::types::VariableKind::Regular => "Variable",
                    };

                    diagnostics.push(Diagnostic {
                        range: var.definition.range,
                        severity: Some(DiagnosticSeverity::WARNING),
                        code: Some(NumberOrString::String("unused-variable".to_string())),
                        code_description: None,
                        source: Some("ersa_lsp".to_string()),
                        message: format!("{} '{}' is never used", symbol_type, var.name),
                        related_information: None,
                        tags: Some(vec![DiagnosticTag::UNNECESSARY]),
                        data: None,
                    });
                }
            }
        }

        diagnostics
    }

    fn check_immutable_assignments(&self, uri: &str, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let user_vars = self.user_variables.lock().unwrap();
        let Some(vars) = user_vars.get(uri) else {
            return diagnostics;
        };

        let immutable_vars: std::collections::HashMap<&str, &parser::types::UserVariable> = vars
            .iter()
            .filter(|v| {
                v.var_type
                    .as_ref()
                    .map(|vt| matches!(vt.mutability, parser::types::Mutability::Immutable))
                    .unwrap_or(false)
            })
            .map(|v| (v.name.as_str(), v))
            .collect();

        // Also add built-in constants as immutable
        let constants = data::get_constants();
        let mut all_immutable_vars: std::collections::HashMap<
            &str,
            Option<&parser::types::UserVariable>,
        > = immutable_vars.iter().map(|(k, v)| (*k, Some(*v))).collect();

        for constant in constants {
            all_immutable_vars.insert(constant.as_str(), None);
        }

        if all_immutable_vars.is_empty() {
            return diagnostics;
        }

        let assignments = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_assignments(text)
        };

        for (var_name, line, col) in assignments {
            if let Some(var_info) = all_immutable_vars.get(var_name.as_str()) {
                let (var_kind, related_info) = if let Some(var) = var_info {
                    let kind = match var.kind {
                        parser::types::VariableKind::Define => "define",
                        parser::types::VariableKind::EnumMember => "enum member",
                        parser::types::VariableKind::Regular => {
                            if var.var_type.as_ref().map(|vt| vt.array_dims).unwrap_or(0) > 0 {
                                "const array"
                            } else {
                                "const variable"
                            }
                        }
                    };

                    let info = Some(vec![DiagnosticRelatedInformation {
                        location: tower_lsp::lsp_types::Location {
                            uri: Url::parse(uri).unwrap(),
                            range: var.definition.range,
                        },
                        message: "Defined here as immutable".to_string(),
                    }]);

                    (kind, info)
                } else {
                    // Built-in constant
                    ("built-in constant", None)
                };

                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: line as u32,
                            character: col as u32,
                        },
                        end: Position {
                            line: line as u32,
                            character: (col + var_name.len()) as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("immutable-assignment".to_string())),
                    code_description: None,
                    source: Some("ersa_lsp".to_string()),
                    message: format!(
                        "Cannot assign to {} '{}' because it is immutable",
                        var_kind, var_name
                    ),
                    related_information: related_info,
                    tags: None,
                    data: None,
                });
            }
        }

        diagnostics
    }

    fn check_undefined_variables(&self, uri: &str, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let user_vars = self.user_variables.lock().unwrap();
        let vars = user_vars.get(uri);

        // Build a set of all defined variable names
        let mut defined_vars: std::collections::HashSet<&str> = std::collections::HashSet::new();

        if let Some(vars) = vars {
            for var in vars {
                defined_vars.insert(&var.name);
            }
        }

        // Add built-in constants
        for constant in data::get_constants() {
            defined_vars.insert(constant.as_str());
        }

        // Find all variable references (excluding declarations)
        let var_refs = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_variable_references(text)
        };

        // Also get assignments to check if they're assigning to undefined variables
        let assignments = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_assignments(text)
        };

        // Check variable references
        let mut reported: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (var_name, line, col) in var_refs {
            if !defined_vars.contains(var_name.as_str()) && !reported.contains(&var_name) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: line as u32,
                            character: col as u32,
                        },
                        end: Position {
                            line: line as u32,
                            character: (col + var_name.len()) as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("undefined-variable".to_string())),
                    code_description: None,
                    source: Some("ersa_lsp".to_string()),
                    message: format!("Undefined variable '{}'", var_name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
                reported.insert(var_name);
            }
        }

        // Check assignments to undefined variables
        for (var_name, line, col) in assignments {
            if !defined_vars.contains(var_name.as_str()) && !reported.contains(&var_name) {
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: line as u32,
                            character: col as u32,
                        },
                        end: Position {
                            line: line as u32,
                            character: (col + var_name.len()) as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("undefined-variable".to_string())),
                    code_description: None,
                    source: Some("ersa_lsp".to_string()),
                    message: format!("Cannot assign to undefined variable '{}'", var_name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
                reported.insert(var_name);
            }
        }

        diagnostics
    }

    fn check_undefined_functions(&self, uri: &str, text: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let user_funcs = self.user_functions.lock().unwrap();
        let funcs = user_funcs.get(uri);

        // Build a map of all defined function names to their parameter counts
        let mut defined_funcs: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();

        if let Some(funcs) = funcs {
            for func in funcs {
                defined_funcs.insert(&func.name, func.parameters.len());
            }
        }

        // Add built-in functions
        for builtin in data::get_builtins() {
            defined_funcs.insert(&builtin.name, builtin.parameters.len());
        }

        // Find all function calls
        let func_calls = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_function_calls(text)
        };

        let mut reported: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (func_name, line, col, arg_count) in func_calls {
            if let Some(&expected_param_count) = defined_funcs.get(func_name.as_str()) {
                // Function exists, check parameter count
                if arg_count != expected_param_count {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: line as u32,
                                character: col as u32,
                            },
                            end: Position {
                                line: line as u32,
                                character: (col + func_name.len()) as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("wrong-argument-count".to_string())),
                        code_description: None,
                        source: Some("ersa_lsp".to_string()),
                        message: format!(
                            "Function '{}' expects {} argument{} but got {}",
                            func_name,
                            expected_param_count,
                            if expected_param_count == 1 { "" } else { "s" },
                            arg_count
                        ),
                        related_information: None,
                        tags: None,
                        data: None,
                    });
                }
            } else if !reported.contains(&func_name) {
                // Function doesn't exist
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: line as u32,
                            character: col as u32,
                        },
                        end: Position {
                            line: line as u32,
                            character: (col + func_name.len()) as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("undefined-function".to_string())),
                    code_description: None,
                    source: Some("ersa_lsp".to_string()),
                    message: format!("Undefined function '{}'", func_name),
                    related_information: None,
                    tags: None,
                    data: None,
                });
                reported.insert(func_name);
            }
        }

        diagnostics
    }
}

fn is_upper_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    let chars: Vec<char> = name.chars().collect();

    for (i, ch) in chars.iter().enumerate() {
        if !ch.is_uppercase() && !ch.is_numeric() && *ch != '_' {
            return false;
        }

        if *ch == '_' && i > 0 && chars.get(i - 1) == Some(&'_') {
            return false;
        }
    }

    !name.starts_with('_') && !name.ends_with('_')
}

fn might_be_constant(name: &str) -> bool {
    let has_uppercase = name.chars().any(|c| c.is_uppercase());
    let common_constant_prefixes = ["MAX", "MIN", "DEFAULT", "CONST"];

    has_uppercase
        || common_constant_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
}
