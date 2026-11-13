//! The Markdown of knowledge base articles, read into lines of spans that `explain`
//! (with terminal colors) and the TUI (with its own styles) both draw.
//!
//! Articles use a small part of Markdown: `#` headings, fenced code blocks, and in
//! the text `code` and **bold**.

/// What a line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// A heading (`## Two clocks`).
    Heading,
    /// A line inside a fenced code block, shown as is.
    Code,
    /// Any other line, empty ones included.
    Text,
}

/// How a span of text is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    /// Normal text.
    Plain,
    /// `code`.
    Code,
    /// **bold**.
    Bold,
}

/// A piece of a line with one emphasis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// The text, without its Markdown markers.
    pub text: String,
    /// How it is shown.
    pub emphasis: Emphasis,
}

/// A line of an article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// What the line is.
    pub kind: LineKind,
    /// Its pieces, in order.
    pub spans: Vec<Span>,
}

/// Reads an article body.
pub fn parse(body: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut in_code = false;
    for line in body.lines() {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        lines.push(if in_code {
            Line {
                kind: LineKind::Code,
                spans: vec![plain(line)],
            }
        } else if let Some(heading) = line.strip_prefix('#') {
            Line {
                kind: LineKind::Heading,
                spans: vec![plain(heading.trim_start_matches('#').trim())],
            }
        } else {
            Line {
                kind: LineKind::Text,
                spans: inline(line),
            }
        });
    }
    lines
}

fn plain(text: &str) -> Span {
    Span {
        text: text.to_owned(),
        emphasis: Emphasis::Plain,
    }
}

/// The spans of a text line: `code` first (nothing is bold inside it), then **bold**.
fn inline(line: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    for (i, part) in line.split('`').enumerate() {
        if i % 2 == 1 {
            spans.push(Span {
                text: part.to_owned(),
                emphasis: Emphasis::Code,
            });
            continue;
        }
        for (j, piece) in part.split("**").enumerate() {
            if piece.is_empty() {
                continue;
            }
            spans.push(Span {
                text: piece.to_owned(),
                emphasis: if j % 2 == 1 {
                    Emphasis::Bold
                } else {
                    Emphasis::Plain
                },
            });
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, emphasis: Emphasis) -> Span {
        Span {
            text: text.into(),
            emphasis,
        }
    }

    #[test]
    fn reads_headings_code_blocks_and_inline_markers() {
        let lines = parse("## Two clocks\n\na **TPM** reads `/sys/x`\n```\nraw `line`\n```\n");
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].kind, LineKind::Heading);
        assert_eq!(lines[0].spans, [span("Two clocks", Emphasis::Plain)]);
        assert!(lines[1].spans.is_empty());
        assert_eq!(
            lines[2].spans,
            [
                span("a ", Emphasis::Plain),
                span("TPM", Emphasis::Bold),
                span(" reads ", Emphasis::Plain),
                span("/sys/x", Emphasis::Code),
            ]
        );
        assert_eq!(lines[3].kind, LineKind::Code);
        assert_eq!(lines[3].spans, [span("raw `line`", Emphasis::Plain)]);
    }
}
