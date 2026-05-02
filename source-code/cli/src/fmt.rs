pub fn format_source(source: &str) -> String {
    let mut out = String::new();
    let mut indent = 0i32;

    for line in source.lines() {
        let trimmed = line.trim();

        // skip empty
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        // decrease indent before closing braces
        if trimmed.starts_with('}') || trimmed.starts_with(']') || trimmed.starts_with(')') {
            indent = (indent - 1).max(0);
        }

        // emit indented line
        let spaces = "    ".repeat(indent as usize);
        out.push_str(&spaces);
        out.push_str(trimmed);
        out.push('\n');

        // increase indent after opening braces
        let opens  = trimmed.chars().filter(|&c| c == '{' || c == '[' || c == '(').count() as i32;
        let closes = trimmed.chars().filter(|&c| c == '}' || c == ']' || c == ')').count() as i32;
        indent = (indent + opens - closes).max(0);
    }

    out
}
