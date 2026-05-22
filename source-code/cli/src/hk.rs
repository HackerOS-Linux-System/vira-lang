#[derive(Debug, Clone, PartialEq)]
pub enum HkValue {
    String(String),
    Number(f64),
    Bool(bool),
    Array(Vec<HkValue>),
    Map(Vec<(String, HkValue)>), // Vec zachowuje kolejność
}

impl HkValue {
    pub fn as_str(&self) -> Option<&str> {
        if let HkValue::String(s) = self { Some(s.as_str()) } else { None }
    }
    pub fn as_f64(&self) -> Option<f64> {
        if let HkValue::Number(n) = self { Some(*n) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let HkValue::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_array(&self) -> Option<&Vec<HkValue>> {
        if let HkValue::Array(a) = self { Some(a) } else { None }
    }
    pub fn get(&self, key: &str) -> Option<&HkValue> {
        if let HkValue::Map(map) = self {
            map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else { None }
    }
    pub fn as_string_vec(&self) -> Vec<String> {
        match self {
            HkValue::Array(arr) => arr.iter().filter_map(|v| v.as_str().map(|s| s.to_owned())).collect(),
            HkValue::String(s)  => vec![s.clone()],
            _ => vec![],
        }
    }
}

/// Top-level HK document: ordered list of (section_name, Map)
pub type HkDoc = Vec<(String, HkValue)>;

// ─── Parser ───────────────────────────────────────────────────────────────────

pub fn parse_hk(input: &str) -> Result<HkDoc, String> {
    let lines: Vec<&str> = input.lines().collect();
    let mut doc: HkDoc = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('!') {
            i += 1;
            continue;
        }

        // Section header: [name]
        if line.starts_with('[') {
            let close = line.find(']')
            .ok_or_else(|| format!("Unclosed section at line {}", i + 1))?;
            let section_name = line[1..close].trim().to_owned();

            // Collect all lines until next section
            let start = i + 1;
            let mut end = start;
            while end < lines.len() {
                let next = lines[end].trim();
                if !next.is_empty() && !next.starts_with('!') && next.starts_with('[') {
                    break;
                }
                end += 1;
            }

            let map = parse_map_level(&lines[start..end], 1)
            .map_err(|e| format!("In section [{section_name}]: {e}"))?;
            doc.push((section_name, HkValue::Map(map)));
            i = end;
        } else {
            return Err(format!("Line {}: expected section header [name], found: {line}", i + 1));
        }
    }

    Ok(doc)
}

/// Parse lines at a given nesting level (1 = "->", 2 = "-->", etc.)
fn parse_map_level(lines: &[&str], level: usize) -> Result<Vec<(String, HkValue)>, String> {
    let prefix = "-".repeat(level) + ">";
    let _next_prefix = "-".repeat(level + 1) + ">";
    let mut map: Vec<(String, HkValue)> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();

        if line.is_empty() || line.starts_with('!') {
            i += 1;
            continue;
        }

        // Only process lines starting with our current prefix level
        if !line.starts_with(&prefix) {
            i += 1;
            continue;
        }

        let after = line[prefix.len()..].trim();

        if let Some(arrow_pos) = after.find("=>") {
            // Key-value: -> key => value
            let key   = after[..arrow_pos].trim().to_owned();
            let value = after[arrow_pos + 2..].trim();
            let parsed = parse_value(value)?;

            // Handle dot-notation: a.b.c => val → nested map
            if key.contains('.') && !key.starts_with('.') && !key.ends_with('.') {
                let parts: Vec<&str> = key.splitn(2, '.').collect();
                // Check if we already have an entry for parts[0]
                if let Some(pos) = map.iter().position(|(k, _)| k == parts[0]) {
                    if let HkValue::Map(ref mut sub) = map[pos].1 {
                        insert_dot_key(sub, parts[1], parsed)?;
                    }
                } else {
                    let mut sub = Vec::new();
                    insert_dot_key(&mut sub, parts[1], parsed)?;
                    map.push((parts[0].to_owned(), HkValue::Map(sub)));
                }
            } else {
                map.push((key, parsed));
            }
            i += 1;
        } else {
            // Sub-map header: -> name (no =>)
            let key = after.trim().to_owned();

            // Collect sub-lines at next level
            let sub_start = i + 1;
            let mut sub_end = sub_start;
            while sub_end < lines.len() {
                let sub_line = lines[sub_end].trim();
                if sub_line.is_empty() || sub_line.starts_with('!') {
                    sub_end += 1;
                    continue;
                }
                // Count dashes
                let dashes = sub_line.chars().take_while(|c| *c == '-').count();
                if dashes <= level {
                    break;
                }
                sub_end += 1;
            }

            let sub_map = parse_map_level(&lines[sub_start..sub_end], level + 1)?;
            map.push((key, HkValue::Map(sub_map)));
            i = sub_end;
        }
    }

    Ok(map)
}

fn insert_dot_key(map: &mut Vec<(String, HkValue)>, key: &str, val: HkValue) -> Result<(), String> {
    if key.contains('.') {
        let parts: Vec<&str> = key.splitn(2, '.').collect();
        if let Some(pos) = map.iter().position(|(k, _)| k == parts[0]) {
            if let HkValue::Map(ref mut sub) = map[pos].1 {
                return insert_dot_key(sub, parts[1], val);
            }
        }
        let mut sub = Vec::new();
        insert_dot_key(&mut sub, parts[1], val)?;
        map.push((parts[0].to_owned(), HkValue::Map(sub)));
    } else {
        map.push((key.to_owned(), val));
    }
    Ok(())
}

fn parse_value(s: &str) -> Result<HkValue, String> {
    let s = s.trim();

    // Boolean
    if s.eq_ignore_ascii_case("true")  { return Ok(HkValue::Bool(true)); }
    if s.eq_ignore_ascii_case("false") { return Ok(HkValue::Bool(false)); }

    // Number
    if let Ok(n) = s.parse::<f64>() { return Ok(HkValue::Number(n)); }

    // Array: [1, "two", true]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len()-1];
        let mut items = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut escape = false;
        for ch in inner.chars() {
            if escape { current.push(ch); escape = false; continue; }
            match ch {
                '\\' => escape = true,
                '"'  => { in_quotes = !in_quotes; current.push(ch); }
                ','  if !in_quotes => {
                    let t = current.trim().to_owned();
                    if !t.is_empty() { items.push(parse_value(&t)?); }
                    current.clear();
                }
                _ => current.push(ch),
            }
        }
        let t = current.trim().to_owned();
        if !t.is_empty() { items.push(parse_value(&t)?); }
        return Ok(HkValue::Array(items));
    }

    // Quoted string
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len()-1];
        let mut result = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n')  => result.push('\n'),
                    Some('t')  => result.push('\t'),
                    Some('r')  => result.push('\r'),
                    Some('"')  => result.push('"'),
                    Some('\\') => result.push('\\'),
                    Some(c)    => result.push(c),
                    None       => {}
                }
            } else {
                result.push(c);
            }
        }
        return Ok(HkValue::String(result));
    }

    // Plain string
    Ok(HkValue::String(s.to_owned()))
}

// ─── Lookup helpers ───────────────────────────────────────────────────────────

/// Get a value by path: get(&doc, "package", "name")
pub fn get<'a>(doc: &'a HkDoc, section: &str, key: &str) -> Option<&'a HkValue> {
    doc.iter()
    .find(|(s, _)| s == section)
    .and_then(|(_, v)| v.get(key))
}

pub fn get_str<'a>(doc: &'a HkDoc, section: &str, key: &str) -> Option<&'a str> {
    get(doc, section, key)?.as_str()
}

pub fn get_f64(doc: &HkDoc, section: &str, key: &str) -> Option<f64> {
    get(doc, section, key)?.as_f64()
}

pub fn get_bool(doc: &HkDoc, section: &str, key: &str) -> Option<bool> {
    get(doc, section, key)?.as_bool()
}

pub fn get_str_vec(doc: &HkDoc, section: &str, key: &str) -> Vec<String> {
    get(doc, section, key).map(|v| v.as_string_vec()).unwrap_or_default()
}

// ─── Generator ────────────────────────────────────────────────────────────────

pub fn generate_vira_hk(
    name: &str,
    version: &str,
    description: &str,
    target: &str,
    entry: &str,
    window_title: Option<&str>,
    window_width: Option<u32>,
    window_height: Option<u32>,
    frontend: Option<&str>,
) -> String {
    let mut out = String::new();
    out.push_str("! vira.hk — Vira project manifest\n");
    out.push_str("! Format: HackerOS .hk\n\n");

    out.push_str("[package]\n");
    out.push_str(&format!("-> name        => {name}\n"));
    out.push_str(&format!("-> version     => {version}\n"));
    out.push_str(&format!("-> description => {description}\n"));
    out.push('\n');

    out.push_str("[build]\n");
    out.push_str(&format!("-> entry  => {entry}\n"));
    out.push_str(&format!("-> target => {target}\n"));

    if let Some(title) = window_title {
        out.push('\n');
        out.push_str("[tauri]\n");
        out.push_str(&format!("-> window_title  => {title}\n"));
        if let Some(w) = window_width  { out.push_str(&format!("-> window_width  => {w}\n")); }
        if let Some(h) = window_height { out.push_str(&format!("-> window_height => {h}\n")); }
        if let Some(f) = frontend      { out.push_str(&format!("-> frontend      => {f}\n")); }
    }

    out
}

pub fn generate_project_hk(workspace_name: &str, members: &[&str]) -> String {
    let mut out = String::new();
    out.push_str("! project.hk — Vira Workspace root\n");
    out.push_str("! Format: HackerOS .hk\n\n");

    out.push_str("[workspace]\n");
    out.push_str(&format!("-> name    => {workspace_name}\n"));
    out.push_str("-> version => 0.1.0\n");
    out.push_str(&format!("-> members => [{}]\n",
                          members.iter().map(|m| format!("\"{m}\"")).collect::<Vec<_>>().join(", ")
    ));
    out.push('\n');

    out.push_str("[build]\n");
    out.push_str("-> output => build\n");

    out
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_parse() {
        let input = r#"
        ! comment
        [package]
        -> name => my-app
        -> version => 0.1.0
        -> active => true

        [build]
        -> entry => src/main.vira
        -> target => tauri
        "#;
        let doc = parse_hk(input).unwrap();
        assert_eq!(get_str(&doc, "package", "name"), Some("my-app"));
        assert_eq!(get_str(&doc, "package", "version"), Some("0.1.0"));
        assert_eq!(get_bool(&doc, "package", "active"), Some(true));
        assert_eq!(get_str(&doc, "build", "target"), Some("tauri"));
    }

    #[test]
    fn test_nested_map() {
        let input = r#"
        [tauri]
        -> window
        --> title => HackerApp
        --> width => 1024
        "#;
        let doc = parse_hk(input).unwrap();
        let tauri = doc.iter().find(|(s, _)| s == "tauri").unwrap();
        let win = tauri.1.get("window").unwrap();
        assert_eq!(win.get("title").and_then(|v| v.as_str()), Some("HackerApp"));
    }

    #[test]
    fn test_array() {
        let input = r#"
        [workspace]
        -> members => ["app", "lib", "tools"]
        "#;
        let doc = parse_hk(input).unwrap();
        let members = get_str_vec(&doc, "workspace", "members");
        assert_eq!(members, vec!["app", "lib", "tools"]);
    }

    #[test]
    fn test_dot_notation() {
        let input = r#"
        [config]
        -> db.host => localhost
        -> db.port => 5432
        "#;
        let doc = parse_hk(input).unwrap();
        let config = doc.iter().find(|(s, _)| s == "config").unwrap();
        let db = config.1.get("db").unwrap();
        assert_eq!(db.get("host").and_then(|v| v.as_str()), Some("localhost"));
    }
}
