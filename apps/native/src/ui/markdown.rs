//! The markdown subset the conversation pane renders, ported from
//! `apps/desktop/src/ui/SafeMarkdown.tsx` (`parseMarkdownBlocks`,
//! `tokenizeInlineMarkdown`): paragraphs, headings, fenced code, bullet and
//! numbered lists, and inline code, bold and italic.
//!
//! Nothing here runs or fetches anything. Tables, charts, images and links are
//! later slices; a block this subset does not understand stays visible as text.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(String),
    Em(String),
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
        if let Some(rest) = line.strip_prefix("```") {
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
            if next.trim().is_empty() || next.starts_with("```") || heading(next).is_some() || is_list_line(next) {
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

/// Inline runs: `` `code` ``, `**bold**`, `*italic*` and `_italic_`. A run is
/// only recognised when it closes on the same line, so an unmatched marker stays
/// as the text the author typed.
pub fn parse_inline(text: &str) -> Vec<Inline> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = text.chars().collect();
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
            if let Some(end) = find_pair(&chars, index + 2, "**") {
                flush(&mut buffer, &mut tokens);
                tokens.push(Inline::Strong(chars[index + 2..end].iter().collect()));
                index = end + 2;
                continue;
            }
        }
        if ch == '*' || ch == '_' {
            let marker = ch;
            if let Some(end) = find(&chars, index + 1, marker) {
                if end > index + 1 {
                    flush(&mut buffer, &mut tokens);
                    tokens.push(Inline::Em(chars[index + 1..end].iter().collect()));
                    index = end + 1;
                    continue;
                }
            }
        }
        buffer.push(ch);
        index += 1;
    }
    flush(&mut buffer, &mut tokens);
    tokens
}

fn flush(buffer: &mut String, tokens: &mut Vec<Inline>) {
    if !buffer.is_empty() {
        tokens.push(Inline::Text(std::mem::take(buffer)));
    }
}

fn find(chars: &[char], from: usize, needle: char) -> Option<usize> {
    (from..chars.len()).find(|index| chars[*index] == needle)
}

fn find_pair(chars: &[char], from: usize, needle: &str) -> Option<usize> {
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() || chars.len() < needle.len() {
        return None;
    }
    (from..=chars.len() - needle.len()).find(|start| chars[*start..*start + needle.len()] == needle[..])
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
}
