//! `explain`: the knowledge base in the terminal.

use anstream::println;
use kokuin_kb::{Article, Kb};

use crate::{
    Failure,
    markdown::{self, Emphasis, LineKind},
    output::{paint, style},
};

pub fn run(id: Option<String>, json: bool) -> Result<(), Failure> {
    let kb = Kb::embedded();
    let Some(id) = id else {
        return list(kb, json);
    };
    let article = kb.article(&id).ok_or_else(|| {
        let suggestions: Vec<&str> = kb
            .articles()
            .map(|a| a.id.as_str())
            .filter(|other| other.contains(&id) || id.contains(*other))
            .collect();
        let hint = if suggestions.is_empty() {
            "run `kokuin-cli explain` to list them".to_owned()
        } else {
            format!("did you mean {}?", suggestions.join(", "))
        };
        Failure(format!("no explanation `{id}`: {hint}"))
    })?;
    if json {
        println!("{}", serde_json::to_string_pretty(article)?);
        return Ok(());
    }
    print_article(kb, article);
    Ok(())
}

fn list(kb: &Kb, json: bool) -> Result<(), Failure> {
    if json {
        let summaries: Vec<_> = kb
            .articles()
            .map(|a| serde_json::json!({ "id": a.id, "title": a.title, "summary": a.summary }))
            .collect();
        println!("{}", serde_json::to_string_pretty(&summaries)?);
        return Ok(());
    }
    let mut area = "";
    for article in kb.articles() {
        let article_area = article.id.split('.').next().unwrap_or_default();
        if article_area != area {
            area = article_area;
            println!("{}", paint(style::HEADING, area));
        }
        println!(
            "  {} {}",
            paint(style::ID, format!("{:<30}", article.id)),
            article.title
        );
    }
    Ok(())
}

fn print_article(kb: &Kb, article: &Article) {
    println!("{}", paint(style::TITLE, &article.title));
    println!("{}", paint(style::DIM, &article.summary));
    println!();
    for line in markdown::parse(&article.body) {
        match line.kind {
            LineKind::Heading => println!("{}", paint(style::HEADING, render(&line))),
            LineKind::Code => println!("    {}", paint(style::ID, render(&line))),
            LineKind::Text => println!("{}", render(&line)),
        }
    }
    if !article.related.is_empty() {
        println!();
        println!("{}", paint(style::HEADING, "Related"));
        for id in &article.related {
            let title = kb.article(id).map_or("", |a| a.title.as_str());
            println!("  {} {title}", paint(style::ID, format!("{id:<30}")));
        }
    }
    if !article.references.is_empty() {
        println!();
        println!("{}", paint(style::HEADING, "Learn more"));
        for reference in &article.references {
            println!(
                "  {}: {}",
                reference.title,
                paint(style::DIM, &reference.url)
            );
        }
    }
}

/// The spans of a line, with terminal colors for `code` and **bold**.
fn render(line: &markdown::Line) -> String {
    line.spans
        .iter()
        .map(|span| match span.emphasis {
            Emphasis::Plain => span.text.clone(),
            Emphasis::Code => paint(style::ID, &span.text),
            Emphasis::Bold => paint(style::HEADING, &span.text),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_inline_markdown_without_colors() {
        anstream::ColorChoice::Never.write_global();
        let line = &markdown::parse("a **TPM** reads `/sys/x`")[0];
        let plain = anstream::adapter::strip_str(&render(line)).to_string();
        assert_eq!(plain, "a TPM reads /sys/x");
    }
}
