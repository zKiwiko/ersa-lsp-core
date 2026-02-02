use crate::lsp::LSP;
use std::time::SystemTime;
use tower_lsp::lsp_types::*;

impl LSP {
    pub async fn handle_did_open(&self, params: DidOpenTextDocumentParams) {
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

    pub async fn handle_did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(change) = params.content_changes.into_iter().next() {
            self.documents
                .lock()
                .unwrap()
                .insert(uri.to_string(), change.text.clone());

            let edit_time = SystemTime::now();
            self.last_edit_times
                .lock()
                .unwrap()
                .insert(uri.to_string(), edit_time);

            let uri_clone = uri.to_string();
            let text_clone = change.text.clone();
            let parser = self.parser.clone();
            let user_functions = self.user_functions.clone();
            let user_variables = self.user_variables.clone();
            let last_edit_times = self.last_edit_times.clone();
            let client = self.client.clone();

            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                let should_update = {
                    let times = last_edit_times.lock().unwrap();
                    times.get(&uri_clone).copied() == Some(edit_time)
                };

                if should_update {
                    let functions = {
                        let mut parser = parser.lock().unwrap();
                        parser.extract_user_functions(&text_clone, &uri_clone)
                    };
                    user_functions
                        .lock()
                        .unwrap()
                        .insert(uri_clone.clone(), functions);

                    let variables = {
                        let mut parser = parser.lock().unwrap();
                        parser.extract_user_variables(&text_clone, &uri_clone)
                    };
                    user_variables
                        .lock()
                        .unwrap()
                        .insert(uri_clone.clone(), variables);

                    // Publish diagnostics after updating symbols
                    let mut diagnostics = Vec::new();

                    // Check for duplicate definitions - sort by position first
                    let mut seen_funcs: std::collections::HashMap<
                        String,
                        tower_lsp::lsp_types::Range,
                    > = std::collections::HashMap::new();
                    if let Some(funcs) = user_functions.lock().unwrap().get(&uri_clone) {
                        let mut sorted_funcs: Vec<_> = funcs.iter().collect();
                        sorted_funcs.sort_by_key(|f| {
                            (
                                f.definition.range.start.line,
                                f.definition.range.start.character,
                            )
                        });

                        for func in sorted_funcs {
                            if let Some(first_range) = seen_funcs.get(&func.name) {
                                diagnostics.push(tower_lsp::lsp_types::Diagnostic {
                                    range: func.definition.range,
                                    severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR),
                                    code: Some(tower_lsp::lsp_types::NumberOrString::String(
                                        "duplicate-function".to_string(),
                                    )),
                                    source: Some("ersa_lsp".to_string()),
                                    message: format!(
                                        "Function '{}' is already defined on line {}",
                                        func.name,
                                        first_range.start.line + 1
                                    ),
                                    related_information: Some(vec![
                                        tower_lsp::lsp_types::DiagnosticRelatedInformation {
                                            location: tower_lsp::lsp_types::Location {
                                                uri: tower_lsp::lsp_types::Url::parse(&uri_clone)
                                                    .unwrap(),
                                                range: *first_range,
                                            },
                                            message: "First defined here".to_string(),
                                        },
                                    ]),
                                    ..Default::default()
                                });
                            } else {
                                seen_funcs.insert(func.name.clone(), func.definition.range);
                            }
                        }
                    }

                    let mut seen_vars: std::collections::HashMap<
                        String,
                        tower_lsp::lsp_types::Range,
                    > = std::collections::HashMap::new();
                    if let Some(vars) = user_variables.lock().unwrap().get(&uri_clone) {
                        let mut sorted_vars: Vec<_> = vars.iter().collect();
                        sorted_vars.sort_by_key(|v| {
                            (
                                v.definition.range.start.line,
                                v.definition.range.start.character,
                            )
                        });

                        for var in sorted_vars {
                            if let Some(first_range) = seen_vars.get(&var.name) {
                                diagnostics.push(tower_lsp::lsp_types::Diagnostic {
                                    range: var.definition.range,
                                    severity: Some(tower_lsp::lsp_types::DiagnosticSeverity::ERROR),
                                    code: Some(tower_lsp::lsp_types::NumberOrString::String(
                                        "duplicate-variable".to_string(),
                                    )),
                                    source: Some("ersa_lsp".to_string()),
                                    message: format!(
                                        "Variable '{}' is already defined on line {}",
                                        var.name,
                                        first_range.start.line + 1
                                    ),
                                    related_information: Some(vec![
                                        tower_lsp::lsp_types::DiagnosticRelatedInformation {
                                            location: tower_lsp::lsp_types::Location {
                                                uri: tower_lsp::lsp_types::Url::parse(&uri_clone)
                                                    .unwrap(),
                                                range: *first_range,
                                            },
                                            message: "First defined here".to_string(),
                                        },
                                    ]),
                                    ..Default::default()
                                });
                            } else {
                                seen_vars.insert(var.name.clone(), var.definition.range);
                            }
                        }
                    }

                    client
                        .publish_diagnostics(
                            tower_lsp::lsp_types::Url::parse(&uri_clone).unwrap(),
                            diagnostics,
                            None,
                        )
                        .await;
                }
            });
        }

        if version % 10 == 0 {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("Document changed: {} (v{})", uri, version),
                )
                .await;
        }
    }

    pub async fn handle_did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        let text = self.documents.lock().unwrap().get(&uri).cloned();
        if let Some(text) = text {
            self.publish_diagnostics(&uri, &text).await;
            self.update_user_functions(&uri.to_string(), &text).await;
            self.update_user_variables(&uri.to_string(), &text).await;
        }

        self.client
            .log_message(MessageType::INFO, format!("Document saved: {}", uri))
            .await;
    }

    pub async fn handle_did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.client
            .log_message(MessageType::INFO, format!("Document closed: {}", uri))
            .await;
    }
}
