use crate::lsp::data;
use crate::lsp::parser;
use crate::lsp::utils::{find_closing_paren, split_arguments};
use crate::lsp::LSP;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

fn extract_word_at_position(line: &str, char_pos: usize) -> String {
    let before_cursor: String = line.chars().take(char_pos).collect();
    let after_cursor: String = line.chars().skip(char_pos).collect();

    let word_start = before_cursor
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);

    let word_end_offset = after_cursor
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(after_cursor.len());

    format!(
        "{}{}",
        &before_cursor[word_start..],
        &after_cursor[..word_end_offset]
    )
}

fn is_word_boundary(line: &str, start: usize, end: usize) -> bool {
    let is_start = start == 0
        || !line
            .chars()
            .nth(start - 1)
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);

    let is_end = end >= line.len()
        || !line
            .chars()
            .nth(end)
            .map(|c| c.is_alphanumeric() || c == '_')
            .unwrap_or(false);

    is_start && is_end
}

fn is_position_in_range(pos: Position, range: &Range) -> bool {
    if pos.line < range.start.line || pos.line > range.end.line {
        return false;
    }

    if pos.line == range.start.line && pos.character < range.start.character {
        return false;
    }

    if pos.line == range.end.line && pos.character >= range.end.character {
        return false;
    }

    true
}

impl LSP {
    fn get_document_text(&self, uri: &Url) -> Option<String> {
        self.documents
            .lock()
            .unwrap()
            .get(&uri.to_string())
            .cloned()
    }

    fn uri_string(uri: &Url) -> String {
        uri.to_string()
    }

    fn find_word_positions(&self, lines: &[&str], word: &str) -> Vec<Range> {
        let mut positions = Vec::new();

        for (line_idx, line_text) in lines.iter().enumerate() {
            let mut start = 0;
            while let Some(pos) = line_text[start..].find(word) {
                let actual_pos = start + pos;

                if is_word_boundary(line_text, actual_pos, actual_pos + word.len()) {
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
    }
    pub async fn handle_goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(position.line as usize) else {
            return Ok(None);
        };
        let word = extract_word_at_position(line, position.character as usize);

        if word.is_empty() {
            return Ok(None);
        }

        let uri_str = Self::uri_string(&uri);
        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(func) = user_funcs
            .get(&uri_str)
            .and_then(|funcs| funcs.iter().find(|f| f.name == word))
        {
            if let Ok(func_uri) = Url::parse(&func.definition.uri) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: func_uri,
                    range: func.definition.range,
                })));
            }
        }

        let user_macros = self.user_macros.lock().unwrap();
        if self.features.macros {
            if let Some(mac) = user_macros
                .get(&uri_str)
                .and_then(|macros| macros.iter().find(|m| m.name == word))
            {
                if let Ok(mac_uri) = Url::parse(&mac.definition.uri) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: mac_uri,
                        range: mac.definition.range,
                    })));
                }
            }
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(var) = user_vars
            .get(&uri_str)
            .and_then(|vars| vars.iter().find(|v| v.name == word))
        {
            if let Ok(var_uri) = Url::parse(&var.definition.uri) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: var_uri,
                    range: var.definition.range,
                })));
            }
        }

        // Check imported symbols
        if self.features.imports {
            let imported_funcs = self.imported_functions.lock().unwrap();
            if let Some(func) = imported_funcs
                .get(&uri_str)
                .and_then(|funcs| funcs.iter().find(|f| f.name == word))
            {
                if let Ok(func_uri) = Url::parse(&func.definition.uri) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: func_uri,
                        range: func.definition.range,
                    })));
                }
            }

            if self.features.macros {
                let imported_macros = self.imported_macros.lock().unwrap();
                if let Some(mac) = imported_macros
                    .get(&uri_str)
                    .and_then(|macros| macros.iter().find(|m| m.name == word))
                {
                    if let Ok(mac_uri) = Url::parse(&mac.definition.uri) {
                        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                            uri: mac_uri,
                            range: mac.definition.range,
                        })));
                    }
                }
            }

            let imported_vars = self.imported_variables.lock().unwrap();
            if let Some(var) = imported_vars
                .get(&uri_str)
                .and_then(|vars| vars.iter().find(|v| v.name == word))
            {
                if let Ok(var_uri) = Url::parse(&var.definition.uri) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: var_uri,
                        range: var.definition.range,
                    })));
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

                let before_cursor: String = line.chars().take(char_pos).collect();
                let word_start = before_cursor
                    .rfind(|c: char| !c.is_alphanumeric() && c != '_')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let prefix = &before_cursor[word_start..];

                let is_manual_trigger = params
                    .context
                    .as_ref()
                    .map(|ctx| ctx.trigger_kind == CompletionTriggerKind::INVOKED)
                    .unwrap_or(false);

                if !is_manual_trigger && prefix.len() < 2 {
                    return Ok(None);
                }

                let mut items = Vec::new();

                items.extend(
                    data::KEYWORDS
                        .iter()
                        .filter(|kw| kw.starts_with(prefix))
                        .map(|kw| CompletionItem {
                            label: kw.to_string(),
                            kind: Some(CompletionItemKind::KEYWORD),
                            detail: Some("Language keyword".to_string()),
                            ..Default::default()
                        }),
                );

                items.extend(
                    data::DATATYPES
                        .iter()
                        .filter(|dt| dt.starts_with(prefix))
                        .map(|dt| CompletionItem {
                            label: dt.to_string(),
                            kind: Some(CompletionItemKind::TYPE_PARAMETER),
                            detail: Some("Language data type".to_string()),
                            ..Default::default()
                        }),
                );

                items.extend(
                    data::get_constants()
                        .into_iter()
                        .filter(|c| c.starts_with(prefix))
                        .take(50 - items.len())
                        .map(|c| CompletionItem {
                            label: c.clone(),
                            kind: Some(CompletionItemKind::CONSTANT),
                            detail: Some("Language constant".to_string()),
                            ..Default::default()
                        }),
                );

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
                                let documentation = func
                                    .documentation
                                    .as_ref()
                                    .map(|doc| Documentation::String(doc.clone()))
                                    .or_else(|| {
                                        Some(Documentation::String(
                                            "User-defined function".to_string(),
                                        ))
                                    });

                                items.push(CompletionItem {
                                    label: func.name.clone(),
                                    kind: Some(CompletionItemKind::FUNCTION),
                                    detail: Some(format!("{}({})", func.name, params_str)),
                                    documentation,
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

                if items.len() < 50 && self.features.macros {
                    let user_macros = self.user_macros.lock().unwrap();
                    if let Some(macros) = user_macros.get(&uri.to_string()) {
                        for mac in macros {
                            if mac.name.starts_with(prefix) {
                                let params_str = mac.parameters.join(", ");
                                let documentation = mac
                                    .documentation
                                    .as_ref()
                                    .map(|doc| Documentation::String(doc.clone()))
                                    .or_else(|| {
                                        Some(Documentation::String(
                                            "User-defined macro".to_string(),
                                        ))
                                    });

                                items.push(CompletionItem {
                                    label: mac.name.clone(),
                                    kind: Some(CompletionItemKind::FUNCTION),
                                    detail: Some(format!("define! {}({})", mac.name, params_str)),
                                    documentation,
                                    insert_text: Some(format!("{}(${{1}})", mac.name)),
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
                                    parser::types::VariableKind::Define => {
                                        (CompletionItemKind::CONSTANT, "Define".to_string())
                                    }
                                    parser::types::VariableKind::Regular => (
                                        CompletionItemKind::VARIABLE,
                                        "User-defined variable".to_string(),
                                    ),
                                };

                                let documentation = var
                                    .documentation
                                    .as_ref()
                                    .map(|doc| Documentation::String(doc.clone()));

                                items.push(CompletionItem {
                                    label: var.name.clone(),
                                    kind: Some(kind),
                                    detail: Some(detail),
                                    documentation,
                                    ..Default::default()
                                });
                            }
                            if items.len() >= 50 {
                                break;
                            }
                        }
                    }
                }

                // Add imported functions
                if items.len() < 50 && self.features.imports {
                    let imported_funcs = self.imported_functions.lock().unwrap();
                    if let Some(funcs) = imported_funcs.get(&uri.to_string()) {
                        for func in funcs {
                            if func.name.starts_with(prefix) {
                                let params_str = func.parameters.join(", ");
                                let documentation = func
                                    .documentation
                                    .as_ref()
                                    .map(|doc| Documentation::String(doc.clone()))
                                    .or_else(|| {
                                        Some(Documentation::String(
                                            "Imported function".to_string(),
                                        ))
                                    });

                                items.push(CompletionItem {
                                    label: func.name.clone(),
                                    kind: Some(CompletionItemKind::FUNCTION),
                                    detail: Some(format!("{}({}) (imported)", func.name, params_str)),
                                    documentation,
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

                // Add imported macros
                if items.len() < 50 && self.features.imports && self.features.macros {
                    let imported_macros = self.imported_macros.lock().unwrap();
                    if let Some(macros) = imported_macros.get(&uri.to_string()) {
                        for mac in macros {
                            if mac.name.starts_with(prefix) {
                                let params_str = mac.parameters.join(", ");
                                let documentation = mac
                                    .documentation
                                    .as_ref()
                                    .map(|doc| Documentation::String(doc.clone()))
                                    .or_else(|| {
                                        Some(Documentation::String(
                                            "Imported macro".to_string(),
                                        ))
                                    });

                                items.push(CompletionItem {
                                    label: mac.name.clone(),
                                    kind: Some(CompletionItemKind::FUNCTION),
                                    detail: Some(format!("define! {}({}) (imported)", mac.name, params_str)),
                                    documentation,
                                    insert_text: Some(format!("{}(${{1}})", mac.name)),
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

                // Add imported variables
                if items.len() < 50 && self.features.imports {
                    let imported_vars = self.imported_variables.lock().unwrap();
                    if let Some(vars) = imported_vars.get(&uri.to_string()) {
                        for var in vars {
                            if var.name.starts_with(prefix) {
                                let (kind, detail_str) = match var.kind {
                                    parser::types::VariableKind::EnumMember => {
                                        (CompletionItemKind::ENUM_MEMBER, "Enum member")
                                    }
                                    parser::types::VariableKind::Define => {
                                        (CompletionItemKind::CONSTANT, "Define")
                                    }
                                    parser::types::VariableKind::Regular => {
                                        (CompletionItemKind::VARIABLE, "Variable")
                                    }
                                };

                                let documentation = var
                                    .documentation
                                    .as_ref()
                                    .map(|doc| Documentation::String(doc.clone()));

                                items.push(CompletionItem {
                                    label: var.name.clone(),
                                    kind: Some(kind),
                                    detail: Some(format!("{} (imported)", detail_str)),
                                    documentation,
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
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(position.line as usize) else {
            return Ok(None);
        };
        let word = extract_word_at_position(line, position.character as usize);

        if word.is_empty() {
            return Ok(None);
        }

        let uri_str = Self::uri_string(&uri);
        let create_hover = |content: String| {
            Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: None,
            })
        };

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(func) = user_funcs
            .get(&uri_str)
            .and_then(|funcs| funcs.iter().find(|f| f.name == word))
        {
            let mut content = format!(
                "```gpc\nfunction {}({})\n```",
                func.name,
                func.parameters.join(", ")
            );
            if let Some(doc) = &func.documentation {
                content.push_str("\n\n");
                content.push_str(doc);
            }
            return Ok(create_hover(content));
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(var) = user_vars
            .get(&uri_str)
            .and_then(|vars| vars.iter().find(|v| v.name == word))
        {
            let type_str = var
                .data_type
                .as_ref()
                .map(|dt| format!("{:?}", dt))
                .unwrap_or_else(|| "unknown".to_string());
            let mut content = format!("```gpc\n{} {}\n```", type_str, var.name);
            if let Some(doc) = &var.documentation {
                content.push_str("\n\n");
                content.push_str(doc);
            }
            return Ok(create_hover(content));
        }

        if let Some(func) = data::get_builtins().iter().find(|f| f.name == word) {
            let params_str = func.parameters.join(", ");
            let content = format!(
                "```gpc\n{}({})\n```\n\n{}",
                func.name, params_str, func.description
            );
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: None,
            }));
        }

        Ok(None)
    }

    fn create_function_call_hints(
        &self,
        line: &str,
        line_idx: u32,
        func_name: &str,
        parameters: &[String],
        skip_definition: Option<&Range>,
    ) -> Vec<InlayHint> {
        let mut hints = Vec::new();
        let func_pattern = format!("{}(", func_name);

        if let Some(start_pos) = line.find(&func_pattern) {
            if let Some(def_range) = skip_definition {
                if def_range.start.line == line_idx {
                    return hints;
                }
            }

            let after_paren = start_pos + func_pattern.len();

            if let Some(end_pos) = find_closing_paren(line, after_paren) {
                let args_str = &line[after_paren..end_pos];
                let args = split_arguments(args_str);

                let mut current_pos = after_paren;
                for (idx, arg) in args.iter().enumerate().take(parameters.len()) {
                    let trimmed_arg = arg.trim();
                    if trimmed_arg.is_empty() {
                        continue;
                    }

                    if let Some(arg_pos) = line[current_pos..].find(trimmed_arg) {
                        let absolute_pos = current_pos + arg_pos;

                        hints.push(InlayHint {
                            position: Position {
                                line: line_idx,
                                character: absolute_pos as u32,
                            },
                            label: InlayHintLabel::String(format!("{}: ", parameters[idx])),
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

        hints
    }

    pub async fn handle_inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };

        let lines: Vec<&str> = text.lines().collect();

        let mut hints: Vec<InlayHint> = lines
            .iter()
            .enumerate()
            .flat_map(|(line_idx, line)| {
                data::get_builtins()
                    .iter()
                    .flat_map(|func| {
                        self.create_function_call_hints(
                            line,
                            line_idx as u32,
                            &func.name,
                            &func.parameters,
                            None,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .collect();

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(&uri.to_string()) {
            for (line_idx, line) in lines.iter().enumerate() {
                for func in funcs {
                    hints.extend(self.create_function_call_hints(
                        line,
                        line_idx as u32,
                        &func.name,
                        &func.parameters,
                        Some(&func.definition.range),
                    ));
                }
            }
        }

        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    pub async fn handle_signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(position.line as usize) else {
            return Ok(None);
        };
        let before_cursor: String = line.chars().take(position.character as usize).collect();

        let mut paren_depth = 0;
        let mut func_name_end = None;

        for (i, ch) in before_cursor.chars().rev().enumerate() {
            match ch {
                ')' => paren_depth += 1,
                '(' => {
                    if paren_depth == 0 {
                        func_name_end = Some(before_cursor.len() - i - 1);
                        break;
                    }
                    paren_depth -= 1;
                }
                _ => {}
            }
        }

        let Some(func_end) = func_name_end else {
            return Ok(None);
        };
        let before_paren = &before_cursor[..func_end];
        let func_start = before_paren
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|i| i + 1)
            .unwrap_or(0);
        let func_name = &before_paren[func_start..];

        if func_name.is_empty() {
            return Ok(None);
        }

        let args_str = &before_cursor[func_end + 1..];
        let active_parameter = split_arguments(args_str).len().saturating_sub(1);

        let create_signature = |name: &str, params: &[String], doc: Option<String>| {
            let params_info: Vec<ParameterInformation> = params
                .iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(p.clone()),
                    documentation: None,
                })
                .collect();

            SignatureInformation {
                label: format!("{}({})", name, params.join(", ")),
                documentation: doc.map(Documentation::String),
                parameters: Some(params_info),
                active_parameter: None,
            }
        };

        let create_help = |signature| {
            Some(SignatureHelp {
                signatures: vec![signature],
                active_signature: Some(0),
                active_parameter: Some(active_parameter as u32),
            })
        };

        if let Some(func) = data::get_builtins().iter().find(|f| f.name == func_name) {
            return Ok(create_help(create_signature(
                &func.name,
                &func.parameters,
                Some(func.description.clone()),
            )));
        }

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(func) = user_funcs
            .get(&Self::uri_string(&uri))
            .and_then(|funcs| funcs.iter().find(|f| f.name == func_name))
        {
            return Ok(create_help(create_signature(
                &func.name,
                &func.parameters,
                func.documentation.clone(),
            )));
        }

        Ok(None)
    }

    pub async fn handle_references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(position.line as usize) else {
            return Ok(None);
        };
        let word = extract_word_at_position(line, position.character as usize);

        if word.is_empty() {
            return Ok(None);
        }

        let ranges = self.find_word_positions(&lines, &word);
        if ranges.is_empty() {
            return Ok(None);
        }

        let locations = ranges
            .into_iter()
            .map(|range| Location {
                uri: uri.clone(),
                range,
            })
            .collect();
        Ok(Some(locations))
    }

    pub async fn handle_document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let mut symbols = Vec::new();

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(&uri.to_string()) {
            for func in funcs {
                #[allow(deprecated)]
                let symbol = DocumentSymbol {
                    name: func.name.clone(),
                    detail: Some(format!("({})", func.parameters.join(", "))),
                    kind: SymbolKind::FUNCTION,
                    tags: None,
                    deprecated: None,
                    range: func.definition.range,
                    selection_range: func.definition.range,
                    children: None,
                };
                symbols.push(symbol);
            }
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(&uri.to_string()) {
            for var in vars {
                let kind = match var.kind {
                    parser::types::VariableKind::EnumMember => SymbolKind::ENUM_MEMBER,
                    parser::types::VariableKind::Define => SymbolKind::CONSTANT,
                    parser::types::VariableKind::Regular => SymbolKind::VARIABLE,
                };

                let detail = if let Some(dt) = &var.data_type {
                    Some(format!("{:?}", dt))
                } else {
                    None
                };

                #[allow(deprecated)]
                let symbol = DocumentSymbol {
                    name: var.name.clone(),
                    detail,
                    kind,
                    tags: None,
                    deprecated: None,
                    range: var.definition.range,
                    selection_range: var.definition.range,
                    children: None,
                };
                symbols.push(symbol);
            }
        }

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        }
    }

    pub async fn handle_document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(position.line as usize) else {
            return Ok(None);
        };
        let word = extract_word_at_position(line, position.character as usize);

        if word.is_empty() {
            return Ok(None);
        }

        let uri_str = Self::uri_string(&uri);
        let is_definition_line = |line_idx: usize| -> bool {
            let line = line_idx as u32;
            self.user_functions
                .lock()
                .unwrap()
                .get(&uri_str)
                .map_or(false, |funcs| {
                    funcs
                        .iter()
                        .any(|f| f.name == word && f.definition.range.start.line == line)
                })
                || self
                    .user_variables
                    .lock()
                    .unwrap()
                    .get(&uri_str)
                    .map_or(false, |vars| {
                        vars.iter()
                            .any(|v| v.name == word && v.definition.range.start.line == line)
                    })
        };

        let ranges = self.find_word_positions(&lines, &word);
        if ranges.is_empty() {
            return Ok(None);
        }

        let highlights = ranges
            .into_iter()
            .map(|range| {
                let kind = if is_definition_line(range.start.line as usize) {
                    Some(DocumentHighlightKind::WRITE)
                } else {
                    Some(DocumentHighlightKind::READ)
                };
                DocumentHighlight { range, kind }
            })
            .collect();

        Ok(Some(highlights))
    }

    pub async fn handle_rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let Some(line) = lines.get(position.line as usize) else {
            return Ok(None);
        };
        let old_name = extract_word_at_position(line, position.character as usize);

        if old_name.is_empty() {
            return Ok(None);
        }

        let ranges = self.find_word_positions(&lines, &old_name);
        if ranges.is_empty() {
            return Ok(None);
        }

        let edits = ranges
            .into_iter()
            .map(|range| TextEdit {
                range,
                new_text: new_name.clone(),
            })
            .collect();

        let mut changes = std::collections::HashMap::new();
        changes.insert(uri, edits);

        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    pub async fn handle_folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri;

        let text = {
            let documents = self.documents.lock().unwrap();
            documents.get(&uri.to_string()).cloned()
        };

        let Some(text) = text else {
            return Ok(None);
        };

        let mut ranges = Vec::new();

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(&uri.to_string()) {
            for func in funcs {
                if func.definition.range.start.line < func.definition.range.end.line {
                    ranges.push(FoldingRange {
                        start_line: func.definition.range.start.line,
                        start_character: Some(func.definition.range.start.character),
                        end_line: func.definition.range.end.line,
                        end_character: Some(func.definition.range.end.character),
                        kind: Some(FoldingRangeKind::Region),
                        collapsed_text: Some(format!("{}...", func.name)),
                    });
                }
            }
        }

        let lines: Vec<&str> = text.lines().collect();
        let mut comment_start: Option<u32> = None;

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                if comment_start.is_none() {
                    comment_start = Some(idx as u32);
                }
            } else if let Some(start) = comment_start {
                if idx as u32 - start > 1 {
                    ranges.push(FoldingRange {
                        start_line: start,
                        start_character: None,
                        end_line: idx as u32 - 1,
                        end_character: None,
                        kind: Some(FoldingRangeKind::Comment),
                        collapsed_text: Some("/* ... */".to_string()),
                    });
                }
                comment_start = None;
            }
        }

        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ranges))
        }
    }

    pub async fn handle_code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();
        let mut lenses = Vec::new();

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(&uri.to_string()) {
            for func in funcs {
                let ref_count = self
                    .find_word_positions(&lines, &func.name)
                    .into_iter()
                    .filter(|r| !is_position_in_range(r.start, &func.definition.range))
                    .count();

                let reference_text = if ref_count == 1 {
                    "1 reference".to_string()
                } else {
                    format!("{} references", ref_count)
                };

                lenses.push(CodeLens {
                    range: Range {
                        start: Position {
                            line: func.definition.range.start.line,
                            character: 0,
                        },
                        end: func.definition.range.start,
                    },
                    command: Some(Command {
                        title: reference_text,
                        command: "editor.action.showReferences".to_string(),
                        arguments: None,
                    }),
                    data: None,
                });
            }
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(&uri.to_string()) {
            for var in vars {
                let ref_count = self
                    .find_word_positions(&lines, &var.name)
                    .into_iter()
                    .filter(|r| !is_position_in_range(r.start, &var.definition.range))
                    .count();

                let reference_text = if ref_count == 1 {
                    "1 reference".to_string()
                } else {
                    format!("{} references", ref_count)
                };

                lenses.push(CodeLens {
                    range: Range {
                        start: Position {
                            line: var.definition.range.start.line,
                            character: 0,
                        },
                        end: var.definition.range.start,
                    },
                    command: Some(Command {
                        title: reference_text,
                        command: "editor.action.showReferences".to_string(),
                        arguments: None,
                    }),
                    data: None,
                });
            }
        }

        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }

    pub async fn handle_semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;
        let Some(text) = self.get_document_text(&uri) else {
            return Ok(None);
        };
        let lines: Vec<&str> = text.lines().collect();

        const TOKEN_TYPE_FUNCTION: u32 = 0;
        const TOKEN_TYPE_VARIABLE: u32 = 1;
        const TOKEN_TYPE_ENUM_MEMBER: u32 = 2;
        const TOKEN_TYPE_PARAMETER: u32 = 3;

        const TOKEN_MODIFIER_DECLARATION: u32 = 1 << 0;
        const TOKEN_MODIFIER_READONLY: u32 = 1 << 1;

        let mut tokens_data: Vec<SemanticToken> = Vec::new();
        let mut prev_line = 0;
        let mut prev_start = 0;

        let mut add_token =
            |line: u32, start: u32, length: u32, token_type: u32, modifiers: u32| {
                let delta_line = line - prev_line;
                let delta_start = if delta_line == 0 {
                    start - prev_start
                } else {
                    start
                };

                tokens_data.push(SemanticToken {
                    delta_line,
                    delta_start,
                    length,
                    token_type,
                    token_modifiers_bitset: modifiers,
                });

                prev_line = line;
                prev_start = start;
            };

        let user_funcs = self.user_functions.lock().unwrap();
        if let Some(funcs) = user_funcs.get(&uri.to_string()) {
            for func in funcs {
                add_token(
                    func.definition.range.start.line,
                    func.definition.range.start.character,
                    func.name.len() as u32,
                    TOKEN_TYPE_FUNCTION,
                    TOKEN_MODIFIER_DECLARATION,
                );

                for param in &func.parameters {
                    if let Some(line_text) = lines.get(func.definition.range.start.line as usize) {
                        if let Some(pos) = line_text.find(param.as_str()) {
                            add_token(
                                func.definition.range.start.line,
                                pos as u32,
                                param.len() as u32,
                                TOKEN_TYPE_PARAMETER,
                                TOKEN_MODIFIER_DECLARATION | TOKEN_MODIFIER_READONLY,
                            );
                        }
                    }
                }

                let def_pos = (
                    func.definition.range.start.line,
                    func.definition.range.start.character,
                );
                for range in self.find_word_positions(&lines, &func.name) {
                    let pos = (range.start.line, range.start.character);
                    if pos != def_pos {
                        add_token(
                            range.start.line,
                            range.start.character,
                            func.name.len() as u32,
                            TOKEN_TYPE_FUNCTION,
                            0,
                        );
                    }
                }
            }
        }

        let user_vars = self.user_variables.lock().unwrap();
        if let Some(vars) = user_vars.get(&uri.to_string()) {
            for var in vars {
                let token_type = match var.kind {
                    parser::types::VariableKind::EnumMember => TOKEN_TYPE_ENUM_MEMBER,
                    parser::types::VariableKind::Define => TOKEN_TYPE_VARIABLE,
                    parser::types::VariableKind::Regular => TOKEN_TYPE_VARIABLE,
                };

                let mut modifiers = TOKEN_MODIFIER_DECLARATION;
                if let Some(var_type) = &var.var_type {
                    if matches!(var_type.mutability, parser::types::Mutability::Immutable) {
                        modifiers |= TOKEN_MODIFIER_READONLY;
                    }
                }

                add_token(
                    var.definition.range.start.line,
                    var.definition.range.start.character,
                    var.name.len() as u32,
                    token_type,
                    modifiers,
                );

                let def_pos = (
                    var.definition.range.start.line,
                    var.definition.range.start.character,
                );
                let ref_modifiers = if matches!(
                    var.var_type.as_ref().map(|v| &v.mutability),
                    Some(parser::types::Mutability::Immutable)
                ) {
                    TOKEN_MODIFIER_READONLY
                } else {
                    0
                };

                for range in self.find_word_positions(&lines, &var.name) {
                    let pos = (range.start.line, range.start.character);
                    if pos != def_pos {
                        add_token(
                            range.start.line,
                            range.start.character,
                            var.name.len() as u32,
                            token_type,
                            ref_modifiers,
                        );
                    }
                }
            }
        }

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens_data,
        })))
    }
}
