//! `explain`: the knowledge base in the terminal.

use anstream::println;
use kokuin_kb::{Article, Kb};

use crate::{
    Failure,
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
    let mut in_code = false;
    for line in article.body.lines() {
        if line.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            println!("    {}", paint(style::ID, line));
        } else if let Some(heading) = line.strip_prefix('#') {
            println!(
                "{}",
                paint(style::HEADING, heading.trim_start_matches('#').trim())
            );
        } else {
            println!("{}", inline_markdown(line));
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

/// Renders the inline Markdown of a line: `code` and **bold**.
fn inline_markdown(line: &str) -> String {
    let alternate = |text: &str,
                     marker: &str,
                     styled: &dyn Fn(&str) -> String,
                     plain: &dyn Fn(&str) -> String| {
        text.split(marker)
            .enumerate()
            .map(|(i, part)| {
                if i % 2 == 1 {
                    styled(part)
                } else {
                    plain(part)
                }
            })
            .collect::<String>()
    };
    let bold = |text: &str| {
        alternate(text, "**", &|part| paint(style::HEADING, part), &|part| {
            part.to_owned()
        })
    };
    alternate(line, "`", &|part| paint(style::ID, part), &bold)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_inline_markdown_without_colors() {
        anstream::ColorChoice::Never.write_global();
        let rendered = inline_markdown("a **TPM** reads `/sys/x`");
        let plain = anstream::adapter::strip_str(&rendered).to_string();
        assert_eq!(plain, "a TPM reads /sys/x");
    }
}
