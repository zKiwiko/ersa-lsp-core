/// Basic GPC source formatter, inspired loosely by clang-format style.
///
/// Rules applied:
///   - 4-space indentation per `{}` depth level
///   - At most one consecutive blank line between items
///   - Space after `,`
///   - Space before `{`
///   - Space between control keywords (`if`/`while`/`for`/`switch`/`return`) and `(`
///   - Collapse multiple spaces into one
///   - Strip trailing whitespace

pub fn format_source(source: &str) -> String {
    let mut out = String::with_capacity(source.len() + source.len() / 8);
    let mut depth: i32 = 0;
    let mut blank_run: u32 = 0;

    for raw_line in source.lines() {
        let trimmed = raw_line.trim();

        if trimmed.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;

        let (opens, closes) = count_braces(trimmed);

        // A line that opens with `}` belongs one level up; reduce before printing.
        if trimmed.starts_with('}') {
            depth = (depth - 1).max(0);
        }

        let formatted = format_line(trimmed);
        push_indent(&mut out, depth);
        out.push_str(&formatted);
        out.push('\n');

        // Net brace change for the next line.
        // If the line opened with `}`, that close was already consumed above,
        // so subtract it from the count to avoid double-counting.
        let net = if trimmed.starts_with('}') {
            opens - (closes - 1)
        } else {
            opens - closes
        };
        depth = (depth + net).max(0);
    }

    // Remove trailing blank lines, keep exactly one trailing newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }

    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn push_indent(out: &mut String, depth: i32) {
    for _ in 0..depth.max(0) {
        out.push_str("    ");
    }
}

/// Count `{` and `}` outside string literals and line comments.
fn count_braces(line: &str) -> (i32, i32) {
    let bytes = line.as_bytes();
    let mut opens = 0i32;
    let mut closes = 0i32;
    let mut i = 0;
    let mut in_str = false;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    break;
                }
            }
            b'{' if !in_str => opens += 1,
            b'}' if !in_str => closes += 1,
            _ => {}
        }
        i += 1;
    }

    (opens, closes)
}

/// Format a single trimmed line, applying spacing rules to the code portion
/// while leaving string literals and the trailing line comment unchanged.
fn format_line(line: &str) -> String {
    let (code, comment) = split_line_comment(line);
    let code = code.trim_end();
    let formatted = if code.is_empty() {
        String::new()
    } else {
        apply_spacing(code)
    };

    match (formatted.is_empty(), comment.is_empty()) {
        (true, true) => String::new(),
        (true, false) => comment.trim_start_matches(' ').to_string(),
        (false, true) => formatted,
        (false, false) => format!("{} {}", formatted, comment.trim_start()),
    }
}

/// Split `line` into `(code, comment)` at the first `//` outside a string.
/// The comment part retains its `//` prefix.
fn split_line_comment(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_str = false;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'/' if !in_str => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    return (&line[..i], &line[i..]);
                }
            }
            _ => {}
        }
        i += 1;
    }

    (line, "")
}

/// Apply all spacing rules to the code portion of a line.
///
/// This is a single character-level pass so that spacing decisions are always
/// string-aware and can look back at what has already been emitted.
fn apply_spacing(code: &str) -> String {
    let bytes = code.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + 8);
    let mut i = 0;
    let mut in_str = false;

    while i < len {
        let b = bytes[i];

        // Inside a string literal – emit verbatim.
        if in_str {
            out.push(b as char);
            if b == b'\\' && i + 1 < len {
                i += 1;
                out.push(bytes[i] as char);
            } else if b == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }

        match b {
            b'"' => {
                out.push('"');
                in_str = true;
            }

            b',' => {
                out.push(',');
                // Consume any spaces already present and emit exactly one.
                while i + 1 < len && bytes[i + 1] == b' ' {
                    i += 1;
                }
                // Only add the space if there is more content (avoid trailing space).
                if i + 1 < len {
                    out.push(' ');
                }
            }

            b'(' => {
                // Control keywords need a space before `(`.
                if trailing_word_is_keyword(out.as_bytes()) {
                    out.push(' ');
                }
                out.push('(');
            }

            b'{' => {
                // Always one space before `{`.
                if !matches!(out.as_bytes().last(), Some(b' ') | Some(b'\t') | None) {
                    out.push(' ');
                }
                out.push('{');
            }

            b' ' | b'\t' => {
                // Collapse whitespace runs to a single space.
                out.push(' ');
                while i + 1 < len && matches!(bytes[i + 1], b' ' | b'\t') {
                    i += 1;
                }
            }

            _ => out.push(b as char),
        }

        i += 1;
    }

    out
}

/// Return `true` if the last identifier already written into `buf` is one of
/// the keywords that should be followed by a space before `(`.
fn trailing_word_is_keyword(buf: &[u8]) -> bool {
    let end = buf.len();
    // Walk back to find the start of the last run of identifier chars.
    let start = buf
        .iter()
        .rposition(|&b| !b.is_ascii_alphanumeric() && b != b'_')
        .map(|p| p + 1)
        .unwrap_or(0);

    if start >= end {
        return false;
    }

    // The character just before the identifier must not itself be an identifier
    // char (sanity-check word boundary on the left side).
    if start > 0 {
        let before = buf[start - 1];
        if before.is_ascii_alphanumeric() || before == b'_' {
            return false;
        }
    }

    matches!(
        &buf[start..end],
        b"if" | b"while" | b"for" | b"switch" | b"return"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indentation() {
        let src = "function foo() {\nint x = 1;\n}\n";
        let out = format_source(src);
        assert_eq!(out, "function foo() {\n    int x = 1;\n}\n");
    }

    #[test]
    fn test_space_after_comma() {
        let src = "foo(a,b,c);\n";
        let out = format_source(src);
        assert!(out.contains("foo(a, b, c)"));
    }

    #[test]
    fn test_keyword_paren_space() {
        let src = "if(x) {\n}\n";
        let out = format_source(src);
        assert!(out.contains("if (x)"));
    }

    #[test]
    fn test_space_before_brace() {
        let src = "function foo(){\n}\n";
        let out = format_source(src);
        assert!(out.contains("foo() {"));
    }

    #[test]
    fn test_blank_line_limit() {
        let src = "int a = 1;\n\n\n\nint b = 2;\n";
        let out = format_source(src);
        assert!(!out.contains("\n\n\n"));
    }

    #[test]
    fn test_string_preserved() {
        let src = "foo(\"if(x),hello\");\n";
        let out = format_source(src);
        // String contents must not be modified.
        assert!(out.contains("\"if(x),hello\""));
    }

    #[test]
    fn test_else_block() {
        let src = "if (x) {\nfoo();\n} else {\nbar();\n}\n";
        let out = format_source(src);
        assert!(out.contains("} else {"));
        // foo and bar should be indented once.
        assert!(out.contains("    foo()"));
        assert!(out.contains("    bar()"));
    }
}
