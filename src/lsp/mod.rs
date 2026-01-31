pub mod data;
pub mod parser;
mod tree_visitor;

use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

pub struct LSP {
    client: Client,
    documents: Arc<Mutex<HashMap<String, String>>>,
    parser: Arc<Mutex<parser::GpcParser>>,
    user_functions: Arc<Mutex<HashMap<String, Vec<parser::UserFunction>>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for LSP {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, "Ersa LSP initializing...")
            .await;

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        will_save: None,
                        will_save_wait_until: None,
                        save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                    },
                )),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: None,
                    all_commit_characters: None,
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    completion_item: None,
                }),
                definition_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("ersa_lsp".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "ersa_lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Ersa LSP initialized successfully.")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Self::log(&self, "Ersa LSP shutting down.").await;
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text;

        self.documents
            .lock()
            .unwrap()
            .insert(uri.clone(), text.clone());

        self.update_user_functions(&uri, &text).await;
        self.publish_diagnostics(&uri, &text).await;

        self.client
            .log_message(
                MessageType::INFO,
                format!("Document opened: {} ({} chars)", uri, text.len()),
            )
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(change) = params.content_changes.into_iter().next() {
            self.documents
                .lock()
                .unwrap()
                .insert(uri.to_string(), change.text.clone());

            // Update user functions only (no diagnostics while typing)
            self.update_user_functions(&uri.to_string(), &change.text)
                .await;
        }

        // Log only occasionally to avoid spam
        if version % 10 == 0 {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("Document changed: {} (v{})", uri, version),
                )
                .await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        // Get the document content and publish diagnostics on save
        let text = self.documents.lock().unwrap().get(&uri).cloned();
        if let Some(text) = text {
            self.publish_diagnostics(&uri, &text).await;
        }

        self.client
            .log_message(MessageType::INFO, format!("Document saved: {}", uri))
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.client
            .log_message(MessageType::INFO, format!("Document closed: {}", uri))
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let text = {
            let documents = self.documents.lock().unwrap();
            documents.get(&uri.to_string()).cloned()
        };

        if let Some(text) = text {
            let lines: Vec<&str> = text.lines().collect();
            if (position.line as usize) < lines.len() {
                let line = lines[position.line as usize];
                let char_pos = position.character as usize;

                // Extract the word being typed
                let before_cursor: String = line.chars().take(char_pos).collect();
                let word_start = before_cursor
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let prefix = &before_cursor[word_start..];

                // Check if completion should be triggered
                let is_manual_trigger = params
                    .context
                    .as_ref()
                    .map(|ctx| ctx.trigger_kind == CompletionTriggerKind::INVOKED)
                    .unwrap_or(false);

                // For automatic triggers, require at least 2 characters
                if !is_manual_trigger && prefix.len() < 2 {
                    return Ok(None);
                }

                let mut items = Vec::new();

                for &keyword in data::KEYWORDS {
                    if keyword.starts_with(prefix) {
                        items.push(CompletionItem {
                            label: keyword.to_string(),
                            kind: Some(CompletionItemKind::KEYWORD),
                            detail: Some("Language keyword".to_string()),
                            ..Default::default()
                        });
                    }
                }

                for &datatype in data::DATATYPES {
                    if datatype.starts_with(prefix) {
                        items.push(CompletionItem {
                            label: datatype.to_string(),
                            kind: Some(CompletionItemKind::TYPE_PARAMETER),
                            detail: Some("Language data type".to_string()),
                            ..Default::default()
                        });
                    }
                }

                for constant in data::get_constants() {
                    if constant.starts_with(prefix) {
                        items.push(CompletionItem {
                            label: constant.to_string(),
                            kind: Some(CompletionItemKind::CONSTANT),
                            detail: Some("Language constant".to_string()),
                            ..Default::default()
                        });
                    }
                    if items.len() >= 50 {
                        break;
                    }
                }

                if items.len() < 50 {
                    for func in data::get_builtins() {
                        if func.name.starts_with(prefix) {
                            let params_str = func.parameters.join(", ");
                            items.push(CompletionItem {
                                label: func.name.clone(),
                                kind: Some(CompletionItemKind::FUNCTION),
                                detail: Some(format!("{}({})", func.name, params_str)),
                                documentation: Some(Documentation::String(
                                    func.description.clone(),
                                )),
                                insert_text: Some(format!("{}(${{1}})", func.name)),
                                insert_text_format: Some(InsertTextFormat::SNIPPET),
                                ..Default::default()
                            });
                        }
                        if items.len() >= 50 {
                            break;
                        }
                    }
                }

                if items.len() < 50 {
                    let user_funcs = self.user_functions.lock().unwrap();
                    if let Some(funcs) = user_funcs.get(&uri.to_string()) {
                        for func in funcs {
                            if func.name.starts_with(prefix) {
                                let params_str = func.parameters.join(", ");
                                items.push(CompletionItem {
                                    label: func.name.clone(),
                                    kind: Some(CompletionItemKind::FUNCTION),
                                    detail: Some(format!("{}({})", func.name, params_str)),
                                    documentation: Some(Documentation::String(
                                        "User-defined function".to_string(),
                                    )),
                                    insert_text: Some(format!("{}(${{1}})", func.name)),
                                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                                    ..Default::default()
                                });
                            }
                            if items.len() >= 50 {
                                break;
                            }
                        }
                    }
                }

                if items.len() < 50 {
                    for snippet in data::get_snippets() {
                        if snippet.name.starts_with(prefix) {
                            items.push(CompletionItem {
                                label: snippet.name.clone(),
                                kind: Some(CompletionItemKind::SNIPPET),
                                detail: Some("Code snippet".to_string()),
                                documentation: Some(Documentation::String(
                                    snippet.description.clone(),
                                )),
                                insert_text: Some(snippet.snippet.clone()),
                                insert_text_format: Some(InsertTextFormat::SNIPPET),
                                ..Default::default()
                            });
                        }
                        if items.len() >= 50 {
                            break;
                        }
                    }
                }

                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let documents = self.documents.lock().unwrap();
        if let Some(text) = documents.get(&uri.to_string()) {
            let lines: Vec<&str> = text.lines().collect();
            if (position.line as usize) < lines.len() {
                let line = lines[position.line as usize];
                let hover_content = format!("Line {}: {}", position.line + 1, line.trim());

                let hover = Hover {
                    contents: HoverContents::Scalar(MarkedString::String(hover_content)),
                    range: None,
                };
                return Ok(Some(hover));
            }
        }

        Ok(None)
    }

    /// Todo: FIX CAUSE HOLY THIS IS BAD.
    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let documents = self.documents.lock().unwrap();

        if let Some(text) = documents.get(&uri.to_string()) {
            let mut hints = Vec::new();
            let lines: Vec<&str> = text.lines().collect();

            for (line_idx, line) in lines.iter().enumerate() {
                // Find function calls in this line
                for func in data::get_builtins() {
                    // Look for pattern: function_name(
                    let func_pattern = format!("{}(", func.name);

                    if let Some(start_pos) = line.find(&func_pattern) {
                        let after_paren = start_pos + func_pattern.len();

                        // Find the matching closing paren
                        if let Some(end_pos) = find_closing_paren(line, after_paren) {
                            // Extract arguments
                            let args_str = &line[after_paren..end_pos];
                            let args: Vec<&str> = split_arguments(args_str);

                            // Create hints for each argument
                            let mut current_pos = after_paren;
                            for (idx, arg) in args.iter().enumerate() {
                                if idx < func.parameters.len() {
                                    let trimmed_arg = arg.trim();
                                    if !trimmed_arg.is_empty() {
                                        // Find position of this argument in the line
                                        if let Some(arg_pos) = line[current_pos..].find(trimmed_arg)
                                        {
                                            let absolute_pos = current_pos + arg_pos;

                                            hints.push(InlayHint {
                                                position: Position {
                                                    line: line_idx as u32,
                                                    character: absolute_pos as u32,
                                                },
                                                label: InlayHintLabel::String(format!(
                                                    "{}: ",
                                                    func.parameters[idx]
                                                )),
                                                kind: Some(InlayHintKind::PARAMETER),
                                                text_edits: None,
                                                tooltip: None,
                                                padding_left: None,
                                                padding_right: None,
                                                data: None,
                                            });

                                            current_pos = absolute_pos + trimmed_arg.len();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            return Ok(Some(hints));
        }

        Ok(None)
    }
}

// Helper function to find the matching closing parenthesis
fn find_closing_paren(text: &str, start: usize) -> Option<usize> {
    let mut depth = 1;
    let chars: Vec<char> = text.chars().collect();

    for (i, &ch) in chars.iter().enumerate().skip(start) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

// Helper function to split arguments respecting nested parentheses
fn split_arguments(args: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut start = 0;

    for (i, ch) in args.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                result.push(&args[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    // Add the last argument
    if start < args.len() {
        result.push(&args[start..]);
    }

    result
}

impl LSP {
    pub fn new(client: Client) -> Self {
        LSP {
            client,
            documents: Arc::new(Mutex::new(HashMap::new())),
            parser: Arc::new(Mutex::new(parser::GpcParser::new())),
            user_functions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start() {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();

        let (service, socket) = LspService::new(|client| LSP::new(client));
        Server::new(stdin, stdout, socket).serve(service).await;
    }

    async fn log(&self, message: &str) {
        self.client
            .log_message(MessageType::INFO, message.to_string())
            .await;
    }

    async fn update_user_functions(&self, uri: &str, text: &str) {
        let functions = {
            let mut parser = self.parser.lock().unwrap();
            parser.extract_user_functions(text)
        }; // Lock is dropped here

        self.user_functions
            .lock()
            .unwrap()
            .insert(uri.to_string(), functions);
    }

    async fn publish_diagnostics(&self, uri: &str, text: &str) {
        let errors = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_syntax_errors(text)
        }; // Lock is dropped here

        let diagnostics: Vec<Diagnostic> = errors
            .into_iter()
            .map(|(line, col, message)| Diagnostic {
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
                code: None,
                code_description: None,
                source: Some("ersa".to_string()),
                message,
                related_information: None,
                tags: None,
                data: None,
            })
            .collect();

        self.client
            .publish_diagnostics(Url::parse(uri).unwrap(), diagnostics, None)
            .await;
    }
}
