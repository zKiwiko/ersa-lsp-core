use crate::lsp::data;
use crate::lsp::parser;
use crate::lsp::utils::{find_closing_paren, split_arguments};
use crate::lsp::LSP;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

impl LSP {
    pub async fn handle_goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let text = {
            let documents = self.documents.lock().unwrap();
            documents.get(&uri.to_string()).cloned()
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let lines: Vec<&str> = text.lines().collect();
        if (position.line as usize) >= lines.len() {
            return Ok(None);
        }

        let line = lines[position.line as usize];
        let char_pos = position.character as usize;

        // Extract the word at cursor
        let before_cursor: String = line.chars().take(char_pos).collect();
        let after_cursor: String = line.chars().skip(char_pos).collect();

        let word_start = before_cursor
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);

        let word_end_offset = after_cursor
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after_cursor.len());

        let word = format!(
            "{}{}",
            &before_cursor[word_start..],
            &after_cursor[..word_end_offset]
        );

        if word.is_empty() {
            return Ok(None);
        }

        // Check user-defined functions
        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(&uri.to_string()) {
            for func in funcs {
                if func.name == word {
                    if let Ok(func_uri) = Url::parse(&func.definition.uri) {
                        let location = Location {
                            uri: func_uri,
                            range: func.definition.range,
                        };
                        return Ok(Some(GotoDefinitionResponse::Scalar(location)));
                    }
                }
            }
        }

        // Check user-defined variables
        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(&uri.to_string()) {
            for var in vars {
                if var.name == word {
                    if let Ok(var_uri) = Url::parse(&var.definition.uri) {
                        let location = Location {
                            uri: var_uri,
                            range: var.definition.range,
                        };
                        return Ok(Some(GotoDefinitionResponse::Scalar(location)));
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn handle_completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
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
                    let user_vars = self.user_variables.lock().unwrap();
                    if let Some(vars) = user_vars.get(&uri.to_string()) {
                        for var in vars {
                            if var.name.starts_with(prefix) {
                                let (kind, detail) = match var.kind {
                                    parser::types::VariableKind::EnumMember => {
                                        (CompletionItemKind::ENUM_MEMBER, "Enum member".to_string())
                                    }
                                    parser::types::VariableKind::Regular => (
                                        CompletionItemKind::VARIABLE,
                                        "User-defined variable".to_string(),
                                    ),
                                };
                                items.push(CompletionItem {
                                    label: var.name.clone(),
                                    kind: Some(kind),
                                    detail: Some(detail),
                                    documentation: None,
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

    pub async fn handle_hover(&self, params: HoverParams) -> Result<Option<Hover>> {
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
    pub async fn handle_inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
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
