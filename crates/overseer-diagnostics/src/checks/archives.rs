//! BA2 archives: report the type breakdown and flag unreadable or unsupported archives

use super::Check;
use crate::context::{ArchiveScan, GameContext};
use crate::finding::{Finding, Severity};
use overseer_core::archive::Ba2Kind;

/// Version that we accept: 1 = FO4 OG, 7/8 = FO4 NG/AE, 2/3 = Starfield
const SUPPORTED_VERSIONS: &[u32] = &[1, 2, 3, 7, 8];

/// Reports the BA2 archive types/counts and flags ones the engine can't use
pub struct Archives;

impl Check for Archives {
    fn id(&self) -> &'static str {
        "archives"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut general = 0usize;
        let mut texture = 0usize;

        for archive in &ctx.archives {
            match &archive.scan {
                ArchiveScan::Header(header) => {
                    match header.kind {
                        Ba2Kind::General => general += 1,
                        Ba2Kind::Texture => texture += 1,
                        Ba2Kind::Other(_) => {}
                    }
                    if !SUPPORTED_VERSIONS.contains(&header.version) {
                        findings.push(Finding::new(
                            Severity::Warning,
                            format!(
                                "`{}` (from `{}`) is an unsupported BA2 version ({})",
                                archive.name, archive.mod_name, header.version
                            ),
                            Some("Fallout 4 reads BA2 versions 1, 7 and 8".to_owned()),
                        ));
                    }
                }
                ArchiveScan::Invalid => findings.push(Finding::new(
                    Severity::Warning,
                    format!(
                        "`{}` (from `{}`) is not a valid BA2 (bad header)",
                        archive.name, archive.mod_name
                    ),
                    Some("The game may fail to load it; re-pack or remove it".to_owned()),
                )),
                ArchiveScan::Unreadable(why) => findings.push(Finding::new(
                    Severity::Warning,
                    format!(
                        "`{}` (from `{}`) could not be read",
                        archive.name, archive.mod_name
                    ),
                    Some(why.clone()),
                )),
            }
        }

        if general + texture > 0 {
            findings.push(Finding::new(
                Severity::Info,
                format!("{general} general + {texture} texture BA2 archives deployed"),
                None,
            ));
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ArchiveInfo;
    use overseer_core::archive::Ba2Header;

    fn info(name: &str, scan: ArchiveScan) -> ArchiveInfo {
        ArchiveInfo {
            name: name.to_owned(),
            mod_name: "ModA".to_owned(),
            scan,
        }
    }

    fn header(version: u32, kind: Ba2Kind) -> ArchiveScan {
        ArchiveScan::Header(Ba2Header {
            version,
            kind,
            file_count: 0,
        })
    }

    fn run(archives: Vec<ArchiveInfo>) -> Vec<Finding> {
        let ctx = GameContext {
            archives,
            ..GameContext::default()
        };
        Archives.run(&ctx)
    }

    #[test]
    fn no_archives_reports_nothing() {
        assert!(run(vec![]).is_empty());
    }

    #[test]
    fn counts_general_and_texture_as_one_info_line() {
        let findings = run(vec![
            info("A - Main.ba2", header(1, Ba2Kind::General)),
            info("A - Textures.ba2", header(1, Ba2Kind::Texture)),
            info("B - Textures.ba2", header(1, Ba2Kind::Texture)),
        ]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].title.contains("1 general"));
        assert!(findings[0].title.contains("2 texture"));
    }

    #[test]
    fn an_invalid_archive_warns() {
        let findings = run(vec![info("Bad.ba2", ArchiveScan::Invalid)]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert!(findings[0].title.contains("not a valid BA2"));
    }

    #[test]
    fn an_unreadable_archive_warns_with_the_reason() {
        let findings = run(vec![info(
            "Locked.ba2",
            ArchiveScan::Unreadable("permission denied".to_owned()),
        )]);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
        assert_eq!(findings[0].detail.as_deref(), Some("permission denied"));
    }

    #[test]
    fn an_unsupported_version_warns() {
        let findings = run(vec![info("Weird.ba2", header(99, Ba2Kind::General))]);
        assert!(
            findings.iter().any(|f| f.severity == Severity::Warning
                && f.title.contains("unsupported BA2 version (99)"))
        );
    }

    #[test]
    fn supported_next_gen_versions_do_not_warn() {
        let findings = run(vec![
            info("v7.ba2", header(7, Ba2Kind::Texture)),
            info("v8.ba2", header(8, Ba2Kind::General)),
        ]);
        assert!(findings.iter().all(|f| f.severity != Severity::Warning));
    }

    #[test]
    fn an_other_tag_is_not_counted_in_the_buckets() {
        // A console GNMF archive parses as `Other` — neither general nor texture, and v1
        // is supported, so a lone one produces no findings at all.
        let findings = run(vec![info(
            "Console.ba2",
            header(1, Ba2Kind::Other(*b"GNMF")),
        )]);
        assert!(findings.is_empty());
    }
}
