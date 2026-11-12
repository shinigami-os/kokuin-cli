//! Knowledge base of kokuin.
//!
//! - **Articles** explain each step of the boot to someone learning how Linux works.
//!   Every object of a boot record can point to one through its `explain_id`.
//! - **Rules** recognize things in raw data (kernel log lines, process names) and
//!   tell what they are: a better title, the kind of event, the explanation, the
//!   milestone they mark.
//!
//! Both live as plain files in `kb/` at the repository root (Markdown articles in
//! `kb/articles/<area>/<topic>.md`, TOML rules in `kb/rules/`) and are embedded into
//! the binary at build time.

mod article;
mod rules;

use std::{collections::BTreeMap, fmt, sync::OnceLock};

pub use article::{Article, Reference};
pub use rules::{KmsgMatch, KmsgRule, KmsgTag, ProcessRole, ProcessRule};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}

/// The loaded knowledge base.
#[derive(Debug)]
pub struct Kb {
    articles: BTreeMap<String, Article>,
    kmsg_rules: Vec<KmsgRule>,
    process_rules: Vec<ProcessRule>,
}

/// An error in the knowledge base files.
#[derive(Debug)]
pub struct KbError {
    /// The file or rule the error is in.
    pub location: String,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for KbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.location, self.message)
    }
}

impl std::error::Error for KbError {}

impl Kb {
    /// The knowledge base embedded in this binary.
    ///
    /// The embedded files are checked by the tests of this crate, so loading them
    /// cannot fail in a released binary.
    pub fn embedded() -> &'static Kb {
        static KB: OnceLock<Kb> = OnceLock::new();
        KB.get_or_init(|| {
            Kb::load_embedded()
                .unwrap_or_else(|err| panic!("invalid embedded knowledge base: {err}"))
        })
    }

    /// Parses the embedded files, reporting the first error.
    pub fn load_embedded() -> Result<Kb, KbError> {
        let mut articles = BTreeMap::new();
        for (id, source) in embedded::ARTICLES {
            articles.insert((*id).to_owned(), Article::parse(id, source)?);
        }
        Ok(Kb {
            articles,
            kmsg_rules: rules::parse_kmsg_rules(embedded::KMSG_RULES)?,
            process_rules: rules::parse_process_rules(embedded::PROCESS_RULES)?,
        })
    }

    /// The article with this id, if there is one.
    pub fn article(&self, id: &str) -> Option<&Article> {
        self.articles.get(id)
    }

    /// Every article, sorted by id.
    pub fn articles(&self) -> impl Iterator<Item = &Article> {
        self.articles.values()
    }

    /// Rules for kernel log messages, in priority order: the first match wins.
    pub fn kmsg_rules(&self) -> &[KmsgRule] {
        &self.kmsg_rules
    }

    /// The first kernel log rule matching `message`, with what it extracted.
    pub fn match_kmsg(&self, message: &str) -> Option<KmsgMatch<'_>> {
        self.kmsg_rules.iter().find_map(|rule| rule.apply(message))
    }

    /// Rules for processes, in priority order: the first match wins.
    pub fn process_rules(&self) -> &[ProcessRule] {
        &self.process_rules
    }

    /// The first process rule matching one of `names` (command name, executable
    /// name...).
    pub fn match_process<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str> + Clone,
    ) -> Option<&ProcessRule> {
        self.process_rules
            .iter()
            .find(|rule| names.clone().into_iter().any(|name| rule.matches(name)))
    }
}
