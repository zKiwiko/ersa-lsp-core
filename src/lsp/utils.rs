/// Helper function to find the matching closing parenthesis
pub fn find_closing_paren(text: &str, start: usize) -> Option<usize> {
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

/// Helper function to split arguments respecting nested parentheses
pub fn split_arguments(args: &str) -> Vec<&str> {
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

    if start < args.len() {
        result.push(&args[start..]);
    }

    result
}
