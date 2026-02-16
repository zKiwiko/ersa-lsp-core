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
        self.update_user_macros(&uri, &text).await;
        self.update_user_variables(&uri, &text).await;
        self.resolve_imports(&uri, &text).await;
        self.resolve_main_file_symbols(&uri).await;
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
            let user_macros = self.user_macros.clone();
            let user_variables = self.user_variables.clone();
            let imported_functions = self.imported_functions.clone();
            let imported_macros = self.imported_macros.clone();
            let imported_variables = self.imported_variables.clone();
            let main_file_functions = self.main_file_functions.clone();
            let main_file_macros = self.main_file_macros.clone();
            let main_file_variables = self.main_file_variables.clone();
            let main_file_cache = self.main_file_cache.clone();
            let workspace_root = self.workspace_root.clone();
            let last_edit_times = self.last_edit_times.clone();
            let client = self.client.clone();
            let features = self.features.clone();

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

                    let macros = if features.macros {
                        let mut parser = parser.lock().unwrap();
                        parser.extract_user_macros(&text_clone, &uri_clone)
                    } else {
                        Vec::new()
                    };
                    user_macros
                        .lock()
                        .unwrap()
                        .insert(uri_clone.clone(), macros);

                    let variables = {
                        let mut parser = parser.lock().unwrap();
                        parser.extract_user_variables(&text_clone, &uri_clone)
                    };
                    user_variables
                        .lock()
                        .unwrap()
                        .insert(uri_clone.clone(), variables);

                    // Resolve imports
                    if features.imports {
                        let import_paths = {
                            let mut parser = parser.lock().unwrap();
                            parser.extract_imports(&text_clone)
                        };

                        let mut all_imported_functions = Vec::new();
                        let mut all_imported_macros = Vec::new();
                        let mut all_imported_variables = Vec::new();

                        for import_path in import_paths {
                            // Resolve import path
                            if let Ok(current_url) = tower_lsp::lsp_types::Url::parse(&uri_clone) {
                                if let Ok(current_path) = current_url.to_file_path() {
                                    if let Some(current_dir) = current_path.parent() {
                                        let resolved = current_dir.join(&import_path);
                                        if let Some(resolved_str) = resolved.to_str() {
                                            if let Ok(imported_text) = std::fs::read_to_string(resolved_str) {
                                                let resolved_uri = format!("file://{}", resolved_str);
                                                
                                                let functions = {
                                                    let mut parser = parser.lock().unwrap();
                                                    parser.extract_user_functions(&imported_text, &resolved_uri)
                                                };
                                                all_imported_functions.extend(functions);

                                                if features.macros {
                                                    let macros = {
                                                        let mut parser = parser.lock().unwrap();
                                                        parser.extract_user_macros(&imported_text, &resolved_uri)
                                                    };
                                                    all_imported_macros.extend(macros);
                                                }

                                                let variables = {
                                                    let mut parser = parser.lock().unwrap();
                                                    parser.extract_user_variables(&imported_text, &resolved_uri)
                                                };
                                                all_imported_variables.extend(variables);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        imported_functions
                            .lock()
                            .unwrap()
                            .insert(uri_clone.clone(), all_imported_functions);
                        
                        imported_macros
                            .lock()
                            .unwrap()
                            .insert(uri_clone.clone(), all_imported_macros);
                        
                        imported_variables
                            .lock()
                            .unwrap()
                            .insert(uri_clone.clone(), all_imported_variables);
                    }

                    // Resolve main file symbols
                    if features.main_file {
                        // Detect main file by walking upward
                        let main_file_path = {
                            let current_url = tower_lsp::lsp_types::Url::parse(&uri_clone).ok();
                            let current_path = current_url.and_then(|u| u.to_file_path().ok());

                            if let Some(current_path) = current_path {
                                let ws_root = workspace_root.lock().unwrap().clone();
                                let ws_path = ws_root
                                    .as_ref()
                                    .and_then(|root| tower_lsp::lsp_types::Url::parse(root).ok())
                                    .and_then(|url| url.to_file_path().ok());

                                let mut dir = current_path.parent().map(|p| p.to_path_buf());
                                let mut found = None;

                                while let Some(current_dir) = dir {
                                    let dir_key = current_dir.to_string_lossy().to_string();

                                    // Check cache
                                    {
                                        let cache = main_file_cache.lock().unwrap();
                                        if let Some(cached) = cache.get(&dir_key) {
                                            found = cached.clone();
                                            break;
                                        }
                                    }

                                    let main_gpc = current_dir.join("main.gpc");
                                    if main_gpc.exists() {
                                        let path = main_gpc.to_string_lossy().to_string();
                                        main_file_cache.lock().unwrap().insert(dir_key, Some(path.clone()));
                                        found = Some(path);
                                        break;
                                    }

                                    if let Some(ref ws_path) = ws_path {
                                        if current_dir == *ws_path {
                                            break;
                                        }
                                    }

                                    let parent = current_dir.parent().map(|p| p.to_path_buf());
                                    if parent.as_ref() == Some(&current_dir) {
                                        break;
                                    }
                                    dir = parent;
                                }

                                // Skip if we ARE the main file
                                if let Some(ref main_path) = found {
                                    if current_path.to_string_lossy() == *main_path {
                                        found = None;
                                    }
                                }

                                found
                            } else {
                                None
                            }
                        };

                        if let Some(main_file_path) = main_file_path {
                            let mut all_mf_functions = Vec::new();
                            let mut all_mf_macros = Vec::new();
                            let mut all_mf_variables = Vec::new();

                            let main_uri = format!("file://{}", main_file_path);
                            if let Ok(main_text) = std::fs::read_to_string(&main_file_path) {
                                let functions = {
                                    let mut p = parser.lock().unwrap();
                                    p.extract_user_functions(&main_text, &main_uri)
                                };
                                all_mf_functions.extend(functions);

                                if features.macros {
                                    let macros = {
                                        let mut p = parser.lock().unwrap();
                                        p.extract_user_macros(&main_text, &main_uri)
                                    };
                                    all_mf_macros.extend(macros);
                                }

                                let variables = {
                                    let mut p = parser.lock().unwrap();
                                    p.extract_user_variables(&main_text, &main_uri)
                                };
                                all_mf_variables.extend(variables);

                                // Follow imports from main.gpc
                                let import_paths = {
                                    let mut p = parser.lock().unwrap();
                                    p.extract_imports(&main_text)
                                };

                                let main_dir = std::path::Path::new(&main_file_path).parent();
                                if let Some(main_dir) = main_dir {
                                    let current_url = tower_lsp::lsp_types::Url::parse(&uri_clone).ok();
                                    let current_path = current_url.and_then(|u| u.to_file_path().ok());

                                    for imp_path in import_paths {
                                        let resolved = main_dir.join(&imp_path);
                                        if let Some(resolved_str) = resolved.to_str() {
                                            // Skip the file we're editing
                                            if let Some(ref cp) = current_path {
                                                if resolved == *cp {
                                                    continue;
                                                }
                                            }

                                            if let Ok(imp_text) = std::fs::read_to_string(resolved_str) {
                                                let resolved_uri = format!("file://{}", resolved_str);

                                                let functions = {
                                                    let mut p = parser.lock().unwrap();
                                                    p.extract_user_functions(&imp_text, &resolved_uri)
                                                };
                                                all_mf_functions.extend(functions);

                                                if features.macros {
                                                    let macros = {
                                                        let mut p = parser.lock().unwrap();
                                                        p.extract_user_macros(&imp_text, &resolved_uri)
                                                    };
                                                    all_mf_macros.extend(macros);
                                                }

                                                let variables = {
                                                    let mut p = parser.lock().unwrap();
                                                    p.extract_user_variables(&imp_text, &resolved_uri)
                                                };
                                                all_mf_variables.extend(variables);
                                            }
                                        }
                                    }
                                }
                            }

                            main_file_functions.lock().unwrap().insert(uri_clone.clone(), all_mf_functions);
                            main_file_macros.lock().unwrap().insert(uri_clone.clone(), all_mf_macros);
                            main_file_variables.lock().unwrap().insert(uri_clone.clone(), all_mf_variables);
                        } else {
                            main_file_functions.lock().unwrap().remove(&uri_clone);
                            main_file_macros.lock().unwrap().remove(&uri_clone);
                            main_file_variables.lock().unwrap().remove(&uri_clone);
                        }
                    }

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
            self.update_user_functions(&uri.to_string(), &text).await;
            self.update_user_macros(&uri.to_string(), &text).await;
            self.update_user_variables(&uri.to_string(), &text).await;
            self.resolve_imports(&uri.to_string(), &text).await;
            self.resolve_main_file_symbols(&uri.to_string()).await;
            self.publish_diagnostics(&uri, &text).await;
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
