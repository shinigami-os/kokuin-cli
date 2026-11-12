//! Checks the knowledge base files: a broken reference here would show up as a
//! missing explanation in the tool.

use std::collections::BTreeSet;

use kokuin_kb::Kb;
use kokuin_model::Stage;

fn kb() -> Kb {
    Kb::load_embedded().unwrap_or_else(|err| panic!("{err}"))
}

#[test]
fn every_referenced_article_exists() {
    let kb = kb();
    let mut missing = BTreeSet::new();
    let mut check = |id: &str, from: String| {
        if kb.article(id).is_none() {
            missing.insert(format!("{id} (referenced by {from})"));
        }
    };
    for rule in kb.kmsg_rules() {
        if let Some(id) = &rule.explain {
            check(id, format!("kmsg rule `{}`", rule.pattern()));
        }
    }
    for rule in kb.process_rules() {
        if let Some(id) = &rule.explain {
            check(id, format!("process rule {:?}", rule.names));
        }
    }
    for article in kb.articles() {
        for id in &article.related {
            check(id, format!("article {}", article.id));
        }
    }
    assert!(
        missing.is_empty(),
        "missing articles:\n{}",
        missing.into_iter().collect::<Vec<_>>().join("\n")
    );
}

#[test]
fn every_stage_has_an_article() {
    let kb = kb();
    for stage in Stage::ALL {
        let name = serde_json::to_value(stage).unwrap();
        let id = format!("stage.{}", name.as_str().unwrap());
        assert!(kb.article(&id).is_some(), "missing article {id}");
    }
}

#[test]
fn articles_have_content_and_distinct_titles() {
    let kb = kb();
    let mut titles = BTreeSet::new();
    for article in kb.articles() {
        assert!(!article.body.is_empty(), "{} has no body", article.id);
        assert!(!article.summary.is_empty(), "{} has no summary", article.id);
        assert!(
            titles.insert(article.title.clone()),
            "two articles are titled {:?}",
            article.title
        );
        assert!(
            !article.related.contains(&article.id),
            "{} lists itself as related",
            article.id
        );
    }
}

#[test]
fn rules_recognize_real_messages() {
    let kb = kb();
    let cases = [
        (
            "Linux version 6.17.7-shinigami (clang version 21.1.0) #1 SMP",
            "Linux 6.17.7-shinigami started",
        ),
        (
            "efi: EFI v2.7 by EDK II",
            "UEFI firmware by EDK II, UEFI specification 2.7",
        ),
        (
            "[drm] Initialized virtio_gpu 0.1.0 for virtio0 on minor 0",
            "Graphics driver virtio_gpu is ready",
        ),
        ("Secure boot disabled", "Secure Boot is disabled"),
        ("secureboot: Secure boot enabled", "Secure Boot is enabled"),
        (
            "Run /sbin/init as init process",
            "Kernel started the first process: /sbin/init",
        ),
        (
            "EXT4-fs (vda2): mounted filesystem 3b9e ro with ordered data mode.",
            "ext4 filesystem mounted from vda2",
        ),
    ];
    for (message, title) in cases {
        let found = kb
            .match_kmsg(message)
            .unwrap_or_else(|| panic!("no rule for {message:?}"));
        assert_eq!(found.title.as_deref(), Some(title));
    }
    assert!(
        kb.match_kmsg("EXT4-fs (vda2): re-mounted 3b9e r/w")
            .unwrap()
            .rule
            .tag
            .is_none()
    );
}
