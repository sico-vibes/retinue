//! The markdown the conversation renders, ported from
//! `apps/desktop/src/ui/SafeMarkdown.tsx`: paragraphs, headings, fenced code,
//! bullet and numbered lists, block quotes and tables, with inline code, bold,
//! italic, links and file references.
//!
//! Nothing here runs or fetches anything. A link is only offered when it is
//! `http` or `https` with no user name or password (`safeHttpUrl`), and a link to
//! a local file becomes a file chip rather than something to open in a browser.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(String),
    Em(String),
    /// A link worth opening: the words, and the checked URL.
    Link { text: String, url: String },
    /// A link to a file on this computer: shown, never opened as a web page.
    LinkText { text: String, href: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    pub depth: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(String),
    Heading { level: usize, text: String },
    Code { language: String, text: String },
    List { ordered: bool, items: Vec<ListItem> },
    Quote(String),
    /// A markdown table: the header row first, then its body rows.
    Table { rows: Vec<Vec<String>> },
}

/// The URL a link may open: `http` or `https`, and nothing that carries a user
/// name or a password (`safeHttpUrl`). Anything else is refused here, not later.
pub fn safe_http_url(value: &str) -> Option<String> {
    let value = value.trim();
    let lower = value.to_lowercase();
    let rest = if let Some(rest) = lower.strip_prefix("https://") {
        if rest.is_empty() {
            return None;
        }
        rest
    } else if let Some(rest) = lower.strip_prefix("http://") {
        if rest.is_empty() {
            return None;
        }
        rest
    } else {
        return None;
    };
    // The authority ends at the first slash, question mark or hash.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    if value.chars().any(char::is_whitespace) {
        return None;
    }
    Some(value.to_owned())
}

/// Whether a line is a table's separator row (`| --- | :--: |`).
pub fn is_table_separator(line: &str) -> bool {
    let cells = table_cells(line);
    if cells.len() < 2 {
        return false;
    }
    cells.iter().all(|cell| {
        let trimmed = cell.trim();
        let trimmed = trimmed.trim_start_matches(':').trim_end_matches(':');
        trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '-')
    })
}

/// The cells of a table row, without the outer pipes.
pub fn table_cells(line: &str) -> Vec<String> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect()
}

/// Two lines of a doc comment is enough to tell a table from a paragraph.
fn starts_a_table(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Split markdown into blocks. An unclosed fence stays readable instead of
/// swallowing the rest of the conversation.
pub fn parse_blocks(source: &str) -> Vec<Block> {
    let source = source.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = source.split('\n').collect();
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("```") {
            let language = rest.trim().to_owned();
            index += 1;
            let mut code = Vec::new();
            let mut closed = false;
            while index < lines.len() {
                if lines[index].trim() == "```" {
                    closed = true;
                    index += 1;
                    break;
                }
                code.push(lines[index]);
                index += 1;
            }
            let mut text = code.join("\n");
            if !closed {
                text = lines[index.saturating_sub(code.len() + 1)..index].join("\n");
            }
            blocks.push(Block::Code { language, text });
            continue;
        }
        if starts_a_table(line) && is_table_separator(lines.get(index + 1).copied().unwrap_or("")) {
            let mut rows = vec![table_cells(line)];
            index += 2;
            while index < lines.len() && starts_a_table(lines[index]) {
                rows.push(table_cells(lines[index]));
                index += 1;
            }
            blocks.push(Block::Table { rows });
            continue;
        }
        if line.trim_start().starts_with('>') {
            let mut quote = Vec::new();
            while index < lines.len() && lines[index].trim_start().starts_with('>') {
                quote.push(lines[index].trim_start().trim_start_matches('>').trim_start().to_owned());
                index += 1;
            }
            blocks.push(Block::Quote(quote.join("\n")));
            continue;
        }
        if let Some(heading) = heading(line) {
            blocks.push(heading);
            index += 1;
            continue;
        }
        if is_list_line(line) {
            let ordered = is_ordered_line(line);
            let mut items = Vec::new();
            while index < lines.len() && is_list_line(lines[index]) {
                let current = lines[index];
                let indent = current.len() - current.trim_start().len();
                let trimmed = current.trim_start();
                let body = if ordered {
                    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
                    if digits > 0 && trimmed[digits..].starts_with(". ") {
                        &trimmed[digits + 2..]
                    } else {
                        strip_bullet(trimmed)
                    }
                } else {
                    strip_bullet(trimmed)
                };
                items.push(ListItem { depth: (indent / 2).min(1), text: body.trim_end().to_owned() });
                index += 1;
            }
            blocks.push(Block::List { ordered, items });
            continue;
        }
        let mut paragraph = vec![line];
        index += 1;
        while index < lines.len() {
            let next = lines[index];
            if next.trim().is_empty()
                || next.trim_start().starts_with("```")
                || heading(next).is_some()
                || is_list_line(next)
                || starts_a_table(next)
                || next.trim_start().starts_with('>')
            {
                break;
            }
            paragraph.push(next);
            index += 1;
        }
        blocks.push(Block::Paragraph(paragraph.join("\n")));
    }
    blocks
}

fn heading(line: &str) -> Option<Block> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if (1..=3).contains(&level) {
        let text = trimmed[level..].trim_start();
        if !text.is_empty() {
            return Some(Block::Heading { level, text: text.to_owned() });
        }
    }
    None
}

fn is_list_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let bullet = trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ");
    bullet || is_ordered_line(line)
}

fn strip_bullet(line: &str) -> &str {
    if line.starts_with("- ") || line.starts_with("* ") || line.starts_with("+ ") {
        &line[2..]
    } else {
        line
    }
}

fn is_ordered_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    !digits.is_empty() && trimmed[digits.len()..].starts_with(". ")
}

/// Inline runs: `` `code` ``, `**bold**`, `*italic*`, `_italic_`, a
/// `[label](url)` link and a bare URL. A run is only recognised when it closes,
/// so an unmatched marker stays as the text the author typed.
pub fn parse_inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut buffer = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '`' {
            if let Some(end) = find(&chars, index + 1, '`') {
                flush(&mut buffer, &mut tokens);
                tokens.push(Inline::Code(chars[index + 1..end].iter().collect()));
                index = end + 1;
                continue;
            }
        }
        if ch == '*' && index + 1 < chars.len() && chars[index + 1] == '*' {
            if let Some(end) = find_pair(&chars, index + 2, &['*', '*']) {
                flush(&mut buffer, &mut tokens);
                tokens.push(Inline::Strong(chars[index + 2..end].iter().collect()));
                index = end + 2;
                continue;
            }
        }
        if ch == '[' {
            if let Some((label, href, next)) = link_at(&chars, index) {
                flush(&mut buffer, &mut tokens);
                match safe_http_url(&href) {
                    Some(url) => tokens.push(Inline::Link { text: label, url }),
                    // A link to a file, or anything that is not a safe URL, stays
                    // visible as its own words rather than disappearing.
                    None => tokens.push(Inline::LinkText { text: label, href }),
                }
                index = next;
                continue;
            }
        }
        if (ch.is_ascii_alphabetic()) && (index == 0 || is_url_break(chars[index - 1])) {
            if let Some((url, next)) = bare_url_at(&chars, index) {
                flush(&mut buffer, &mut tokens);
                match safe_http_url(&url) {
                    Some(safe) => tokens.push(Inline::Link { text: safe.clone(), url: safe }),
                    None => tokens.push(Inline::Text(url)),
                }
                index = next;
                continue;
            }
        }
        if ch == '*' || ch == '_' {
            let marker = ch;
            // `_` only marks emphasis between word boundaries, as the app does.
            let boundary = ch != '_' || index == 0 || !chars[index - 1].is_alphanumeric();
            if boundary {
                if let Some(end) = find(&chars, index + 1, marker) {
                    if end > index + 1 && (ch != '_' || end + 1 >= chars.len() || !chars[end + 1].is_alphanumeric()) {
                        flush(&mut buffer, &mut tokens);
                        tokens.push(Inline::Em(chars[index + 1..end].iter().collect()));
                        index = end + 1;
                        continue;
                    }
                }
            }
        }
        buffer.push(ch);
        index += 1;
    }
    flush(&mut buffer, &mut tokens);
    tokens
}

fn is_url_break(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '(' | '[' | '"' | '\'' | '<' | '=' | ':')
}

/// A bare `http://` or `https://` URL, with trailing punctuation left out.
fn bare_url_at(chars: &[char], start: usize) -> Option<(String, usize)> {
    let rest: String = chars[start..].iter().collect();
    let lower = rest.to_lowercase();
    let prefix = if lower.starts_with("https://") {
        "https://"
    } else if lower.starts_with("http://") {
        "http://"
    } else {
        return None;
    };
    let mut end = start + prefix.len();
    while end < chars.len() && !chars[end].is_whitespace() && !matches!(chars[end], '<' | '>' | '"' | '\'') {
        end += 1;
    }
    // A closing bracket only belongs to the URL when it opened inside it.
    loop {
        let mut trimmed = end;
        while trimmed > start && matches!(chars[trimmed - 1], '.' | ',' | '!' | '?' | ';' | ':') {
            trimmed -= 1;
        }
        for (opening, closing) in [('(', ')'), ('[', ']'), ('{', '}')] {
            if trimmed > start && chars[trimmed - 1] == closing {
                let inside = chars[start..trimmed].iter().filter(|ch| **ch == opening).count();
                let all = chars[start..trimmed].iter().filter(|ch| **ch == closing).count();
                if all > inside {
                    trimmed -= 1;
                }
            }
        }
        if trimmed == end {
            break;
        }
        end = trimmed;
    }
    if end <= start + prefix.len() {
        return None;
    }
    Some((chars[start..end].iter().collect(), end))
}

/// `[label](href)` starting at `index`, if it closes on this line.
fn link_at(chars: &[char], index: usize) -> Option<(String, String, usize)> {
    let close = find(chars, index + 1, ']')?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find(chars, close + 2, ')')?;
    let label: String = chars[index + 1..close].iter().collect();
    let href: String = chars[close + 2..end].iter().collect();
    if label.is_empty() || href.trim().is_empty() {
        return None;
    }
    // A newline inside the run would not be a link.
    if label.contains('\n') || href.contains('\n') || href.contains(' ') {
        return None;
    }
    Some((label, href, end + 1))
}

fn flush(buffer: &mut String, tokens: &mut Vec<Inline>) {
    if !buffer.is_empty() {
        tokens.push(Inline::Text(std::mem::take(buffer)));
    }
}

fn find(chars: &[char], from: usize, needle: char) -> Option<usize> {
    (from..chars.len()).find(|index| chars[*index] == needle)
}

fn find_pair(chars: &[char], from: usize, needle: &[char]) -> Option<usize> {
    if needle.is_empty() || chars.len() < needle.len() {
        return None;
    }
    (from..=chars.len() - needle.len()).find(|start| chars[*start..*start + needle.len()] == *needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_headings_and_lists_become_blocks() {
        let blocks = parse_blocks("# Fix the tests\n\nFirst paragraph\nstill the same paragraph.\n\n- one\n- two\n\n1. first\n2. second");
        assert_eq!(blocks[0], Block::Heading { level: 1, text: "Fix the tests".into() });
        assert_eq!(blocks[1], Block::Paragraph("First paragraph\nstill the same paragraph.".into()));
        assert_eq!(
            blocks[2],
            Block::List { ordered: false, items: vec![ListItem { depth: 0, text: "one".into() }, ListItem { depth: 0, text: "two".into() }] }
        );
        assert_eq!(
            blocks[3],
            Block::List { ordered: true, items: vec![ListItem { depth: 0, text: "first".into() }, ListItem { depth: 0, text: "second".into() }] }
        );
    }

    #[test]
    fn a_fenced_block_keeps_its_lines_and_language() {
        let blocks = parse_blocks("Before\n```rust\nfn main() {}\n```\nAfter");
        assert_eq!(blocks[1], Block::Code { language: "rust".into(), text: "fn main() {}".into() });
        assert_eq!(blocks[2], Block::Paragraph("After".into()));
    }

    #[test]
    fn an_unclosed_fence_does_not_swallow_the_rest_silently() {
        let blocks = parse_blocks("```\nstill code");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], Block::Code { language: String::new(), text: "```\nstill code".into() });
    }

    #[test]
    fn a_markdown_table_keeps_its_header_and_rows() {
        let blocks = parse_blocks("| Build | Time |\n| --- | ---: |\n| lint | 12.4 |\n| test | 98.1 |\n\nAfter");
        assert_eq!(
            blocks[0],
            Block::Table {
                rows: vec![
                    vec!["Build".to_owned(), "Time".to_owned()],
                    vec!["lint".to_owned(), "12.4".to_owned()],
                    vec!["test".to_owned(), "98.1".to_owned()],
                ]
            }
        );
        assert_eq!(blocks[1], Block::Paragraph("After".into()), "the table ends at a blank line");
    }

    #[test]
    fn a_pipe_line_without_a_separator_stays_a_paragraph() {
        let blocks = parse_blocks("| not | a table |");
        assert_eq!(blocks[0], Block::Paragraph("| not | a table |".into()));
    }

    #[test]
    fn a_block_quote_is_its_own_block() {
        let blocks = parse_blocks("> quoted\n> still quoted\nplain");
        assert_eq!(blocks[0], Block::Quote("quoted\nstill quoted".into()));
        assert_eq!(blocks[1], Block::Paragraph("plain".into()));
    }

    #[test]
    fn inline_runs_are_recognised() {
        assert_eq!(
            parse_inline("Run `cargo test` and **check** the *output*"),
            vec![
                Inline::Text("Run ".into()),
                Inline::Code("cargo test".into()),
                Inline::Text(" and ".into()),
                Inline::Strong("check".into()),
                Inline::Text(" the ".into()),
                Inline::Em("output".into()),
            ]
        );
    }

    #[test]
    fn an_unmatched_marker_stays_as_text() {
        assert_eq!(parse_inline("2 * 3 = 6"), vec![Inline::Text("2 * 3 = 6".into())]);
        assert_eq!(parse_inline("a `b"), vec![Inline::Text("a `b".into())]);
    }

    #[test]
    fn a_link_is_only_a_link_when_its_url_is_safe() {
        assert_eq!(
            parse_inline("[the docs](https://example.com/a?b=1)"),
            vec![Inline::Link { text: "the docs".into(), url: "https://example.com/a?b=1".into() }]
        );
        // Script URLs, credentials and other schemes are never offered.
        for bad in [
            "[x](javascript:alert(1))",
            "[x](https://user:pass@example.com/)",
            "[x](file:///C:/Windows/System32)",
            "[x](ftp://example.com)",
        ] {
            let tokens = parse_inline(bad);
            assert!(
                !tokens.iter().any(|token| matches!(token, Inline::Link { .. })),
                "{bad} must not become a link: {tokens:?}"
            );
        }
        assert_eq!(safe_http_url("https://example.com/a"), Some("https://example.com/a".to_owned()));
        assert_eq!(safe_http_url("HTTPS://Example.com"), Some("HTTPS://Example.com".to_owned()));
        assert_eq!(safe_http_url("https://"), None);
        assert_eq!(safe_http_url("mailto:a@b.c"), None);
        assert_eq!(safe_http_url("https://a b.example"), None);
    }

    #[test]
    fn a_bare_url_becomes_a_link_and_leaves_the_full_stop_behind() {
        assert_eq!(
            parse_inline("See https://example.com/docs."),
            vec![
                Inline::Text("See ".into()),
                Inline::Link { text: "https://example.com/docs".into(), url: "https://example.com/docs".into() },
                Inline::Text(".".into()),
            ]
        );
        // A closing bracket that was opened inside the URL stays in it.
        assert_eq!(
            parse_inline("https://en.wikipedia.org/wiki/Foo_(bar)"),
            vec![Inline::Link {
                text: "https://en.wikipedia.org/wiki/Foo_(bar)".into(),
                url: "https://en.wikipedia.org/wiki/Foo_(bar)".into()
            }]
        );
    }
}
