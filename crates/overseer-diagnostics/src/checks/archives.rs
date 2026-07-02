//! BA2 archives: report the type breakdown and flag unreadable or unsupported archives

use super::Check;
use crate::context::{ArchiveScan, GameContext};
use crate::finding::{Finding, Severity};

/// Version that we accept: 1 = FO4 OG, 7/8 = FO4 NG/AE, 2/3 = Starfield
const SUPPORTED_VERSIONS: &[u32] = &[1, 2, 3, 7, 8];
const MAX_ARCHIVES_GNRL: usize = 256;
const MAX_ARCHIVES_DX10: usize = 255;

/// Reports the BA2 archive types/counts and flags ones the engine can't use
pub struct Archives;

impl Check for Archives {
    fn id(&self) -> &'static str {
        "archives"
    }

    fn run(&self, ctx: &GameContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        for archive in &ctx.archives {
            match &archive.scan {
                ArchiveScan::Header(header) => {
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

        let counts = &ctx.loaded_archive_counts;
        if let Some(finding) = limit_finding(counts.gnrl, MAX_ARCHIVES_GNRL, "General") {
            findings.push(finding);
        }
        if let Some(finding) = limit_finding(counts.dx10, MAX_ARCHIVES_DX10, "Texture") {
            findings.push(finding);
        }

        if counts.gnrl + counts.dx10 > 0 {
            findings.push(Finding::new(
                Severity::Info,
                format!(
                    "{}/{} general + {}/{} texture BA2 loaded · versions: {} v1, {} v7/8",
                    counts.gnrl,
                    MAX_ARCHIVES_GNRL,
                    counts.dx10,
                    MAX_ARCHIVES_DX10,
                    counts.v1,
                    counts.vng
                ),
                None,
            ));
        }
        findings
    }
}

fn limit_finding(count: usize, limit: usize, label: &str) -> Option<Finding> {
    let warn = limit * 95 / 100;
    if count > limit {
        Some(Finding::new(
            Severity::Error,
            format!("{label} BA2 archives: {count} / {limit} — over the limit"),
            Some("Unpack or merge archives to reduce the total (don't mix texture and non-texture when merging).".to_owned()),
        ))
    } else if count >= warn {
        Some(Finding::new(
            Severity::Warning,
            format!("{label} BA2 archives: {count} / {limit} — approaching the limit"),
            None,
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ArchiveInfo, LoadedArchiveCounts};
    use overseer_core::archive::{Ba2Header, Ba2Kind};

    fn info(name: &str, scan: ArchiveScan) -> ArchiveInfo {
        ArchiveInfo {
            name: name.to_owned(),
            mod_name: "ModA".to_owned(),
            relative: camino::Utf8Path::new("Data").join(name),
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

    fn run_counts(loaded_archive_counts: LoadedArchiveCounts) -> Vec<Finding> {
        let ctx = GameContext {
            loaded_archive_counts,
            ..GameContext::default()
        };
        Archives.run(&ctx)
    }

    fn limit_findings(findings: &[Finding], label: &str) -> Vec<Finding> {
        findings
            .iter()
            .filter(|f| f.title.starts_with(label))
            .cloned()
            .collect()
    }

    #[test]
    fn no_archives_reports_nothing() {
        assert!(run(vec![]).is_empty());
    }

    #[test]
    fn reports_loaded_counts_and_version_split_as_one_info_line() {
        let findings = run_counts(LoadedArchiveCounts {
            gnrl: 1,
            dx10: 2,
            v1: 2,
            vng: 1,
        });
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Info);
        assert_eq!(
            findings[0].title,
            "1/256 general + 2/255 texture BA2 loaded · versions: 2 v1, 1 v7/8"
        );
    }

    #[test]
    fn over_limit_general_and_texture_are_errors() {
        let general = run_counts(LoadedArchiveCounts {
            gnrl: 257,
            ..LoadedArchiveCounts::default()
        });
        let texture = run_counts(LoadedArchiveCounts {
            dx10: 256,
            ..LoadedArchiveCounts::default()
        });

        let general_limit = limit_findings(&general, "General BA2 archives");
        let texture_limit = limit_findings(&texture, "Texture BA2 archives");

        assert_eq!(general_limit.len(), 1);
        assert_eq!(general_limit[0].severity, Severity::Error);
        assert!(general_limit[0].title.contains("257 / 256"));
        assert_eq!(
            general_limit[0].detail.as_deref(),
            Some(
                "Unpack or merge archives to reduce the total (don't mix texture and non-texture when merging)."
            )
        );

        assert_eq!(texture_limit.len(), 1);
        assert_eq!(texture_limit[0].severity, Severity::Error);
        assert!(texture_limit[0].title.contains("256 / 255"));
    }

    #[test]
    fn exactly_at_limit_and_warn_floor_are_warnings_but_just_under_is_clear() {
        let at_limit = run_counts(LoadedArchiveCounts {
            gnrl: 256,
            ..LoadedArchiveCounts::default()
        });
        let at_floor = run_counts(LoadedArchiveCounts {
            gnrl: 243,
            ..LoadedArchiveCounts::default()
        });
        let just_under = run_counts(LoadedArchiveCounts {
            gnrl: 242,
            ..LoadedArchiveCounts::default()
        });

        let at_limit = limit_findings(&at_limit, "General BA2 archives");
        let at_floor = limit_findings(&at_floor, "General BA2 archives");
        let just_under = limit_findings(&just_under, "General BA2 archives");

        assert_eq!(at_limit.len(), 1);
        assert_eq!(at_limit[0].severity, Severity::Warning);
        assert!(at_limit[0].title.contains("256 / 256"));

        assert_eq!(at_floor.len(), 1);
        assert_eq!(at_floor[0].severity, Severity::Warning);
        assert!(at_floor[0].title.contains("243 / 256"));

        assert!(just_under.is_empty());
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
