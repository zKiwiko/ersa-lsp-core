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

    pub async fn publish_diagnostics(&self, uri: &str, text: &str) {
        let errors = {
            let mut parser = self.parser.lock().unwrap();
            parser.find_syntax_errors(text)
        };

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
                source: Some("ersa_lsp".to_string()),
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
