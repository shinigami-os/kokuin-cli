use serde::{Deserialize, Serialize};

use crate::KbError;

/// An explanation of one boot step, component or concept.
///
/// Source format: TOML front matter between `+++` lines, then a Markdown body.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Article {
    /// Identifier, from the file path: `articles/firmware/fpdt.md` is `firmware.fpdt`.
    pub id: String,
    /// Title.
    pub title: String,
    /// One-sentence summary.
    pub summary: String,
    /// Identifiers of related articles.
    pub related: Vec<String>,
    /// Where to learn more.
    pub references: Vec<Reference>,
    /// The explanation, in Markdown.
    pub body: String,
}

/// A link to an external document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    /// Title of the document.
    pub title: String,
    /// Its URL.
    pub url: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FrontMatter {
    title: String,
    summary: String,
    #[serde(default)]
    related: Vec<String>,
    #[serde(default)]
    references: Vec<Reference>,
}

const DELIMITER: &str = "+++";

impl Article {
    /// Parses the source of the article `id`.
    pub(crate) fn parse(id: &str, source: &str) -> Result<Article, KbError> {
        let error = |message: String| KbError {
            location: format!("article {id}"),
            message,
        };

        let rest = source
            .strip_prefix(DELIMITER)
            .ok_or_else(|| error(format!("must start with a `{DELIMITER}` front matter line")))?;
        let (front, body) = rest.split_once(&format!("\n{DELIMITER}")).ok_or_else(|| {
            error(format!(
                "front matter is not closed by a `{DELIMITER}` line"
            ))
        })?;
        let front: FrontMatter = toml::from_str(front).map_err(|err| error(err.to_string()))?;

        Ok(Article {
            id: id.to_owned(),
            title: front.title,
            summary: front.summary,
            related: front.related,
            references: front.references,
            body: body.trim().to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_front_matter_and_body() {
        let source =
            "+++\ntitle = \"T\"\nsummary = \"S\"\nrelated = [\"a.b\"]\n+++\n\nBody *text*.\n";
        let article = Article::parse("x.y", source).unwrap();
        assert_eq!(article.title, "T");
        assert_eq!(article.related, ["a.b"]);
        assert_eq!(article.body, "Body *text*.");
    }

    #[test]
    fn rejects_unknown_front_matter_keys() {
        let source = "+++\ntitle = \"T\"\nsummary = \"S\"\ntypo = 1\n+++\nBody";
        assert!(Article::parse("x.y", source).is_err());
    }
}
