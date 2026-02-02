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
