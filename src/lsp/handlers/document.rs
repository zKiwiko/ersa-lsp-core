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

            // Update timestamp for this document
            let edit_time = SystemTime::now();
            self.last_edit_times
                .lock()
                .unwrap()
                .insert(uri.to_string(), edit_time);

            // Spawn debounced update task
            let uri_clone = uri.to_string();
            let text_clone = change.text.clone();
            let parser = self.parser.clone();
            let user_functions = self.user_functions.clone();
            let user_variables = self.user_variables.clone();
            let last_edit_times = self.last_edit_times.clone();

            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

                // Check if a newer edit happened
                let should_update = {
                    let times = last_edit_times.lock().unwrap();
                    times.get(&uri_clone).copied() == Some(edit_time)
                };

                if should_update {
                    // Extract functions
                    let functions = {
                        let mut parser = parser.lock().unwrap();
                        parser.extract_user_functions(&text_clone, &uri_clone)
                    };
                    user_functions
                        .lock()
                        .unwrap()
                        .insert(uri_clone.clone(), functions);

                    // Extract variables
                    let variables = {
                        let mut parser = parser.lock().unwrap();
                        parser.extract_user_variables(&text_clone, &uri_clone)
                    };
                    user_variables.lock().unwrap().insert(uri_clone, variables);
                }
            });
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

    pub async fn handle_did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();

        // Get the document content and publish diagnostics on save
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
