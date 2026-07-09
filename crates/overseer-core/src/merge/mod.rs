//! The Bethesda archive merge engine: merge BA2 archives and create carrier ESL

use crate::archive::Ba2Kind;
use crate::ba2::{self, Ba2File, Ba2IoError, Ba2Texture};
use crate::error::IoError;
use crate::fs;
use crate::game::GameKind;
use crate::plugins::carrier_for;
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeMap;
use std::collections::btree_map::Entry as MapEntry;
use std::ops::Range;
use thiserror::Error;

/// Default uncompressed bytes per texture archive before splitting; raise it for stronger compressors
pub const DEFAULT_TEXTURE_GROUP_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// A source archive to merge, with an override rank for conflict res
#[derive(Debug, Clone)]
pub struct MergeSource {
    /// The Ba2 to read
    pub archive: Utf8PathBuf,
    /// Larger wins on a path clash
    pub override_rank: usize,
}

/// Options for a merge run
#[derive(Debug, Clone)]
pub struct MergeOptions {
    /// Plugin stem shared by every emitted archive and carrier
    pub basename: String,
    /// Uncompressed byte cap per texture group before it is split
    pub texture_group_bytes: u64,
}

/// One path that appeared in more than one source, and which source won
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeConflict {
    /// The archive-internal path that clashed
    pub path: String,
    /// The source kept
    pub winner: Utf8PathBuf,
    /// The source dropped
    pub loser: Utf8PathBuf,
}

/// One archive written by a merge, paired with the carrier basename that loads it
#[derive(Debug, Clone)]
pub struct MergedArchive {
    /// Full path to staged `.ba2`
    pub archive: Utf8PathBuf,
    /// Carrier plugin basename, without the `.esl` suffix
    pub carrier: String,
    /// General or textures
    pub kind: Ba2Kind,
}

/// Everything a merge staged, ready for a later deploy stage to place
#[derive(Debug)]
pub struct MergeOutput {
    /// Merged archives in emit order: the general archive, then texture groups
    pub archives: Vec<MergedArchive>,
    /// Carrier ESL paths, one per archive
    pub carriers: Vec<Utf8PathBuf>,
    /// Loose STRING paths staged under `Strings/`
    pub strings: Vec<Utf8PathBuf>,
    /// Path clashes resolved by override rank
    pub conflicts: Vec<MergeConflict>,
}

/// How many archives of each kind a merge produced
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MergeCounts {
    /// General archives
    pub gnrl: usize,
    /// Texture archives
    pub dx10: usize,
}

impl MergeOutput {
    /// Tally the emitted archives by kind
    pub fn counts(&self) -> MergeCounts {
        let dx10 = self
            .archives
            .iter()
            .filter(|a| a.kind == Ba2Kind::Texture)
            .count();
        MergeCounts {
            gnrl: self.archives.len() - dx10,
            dx10,
        }
    }
}

/// Why a merge could not complete
#[derive(Debug, Error)]
pub enum MergeError {
    /// Reading or repacking a BA2 failed
    #[error(transparent)]
    Ba2(#[from] Ba2IoError),

    /// A staging write failed
    #[error(transparent)]
    Io(#[from] IoError),

    /// No sources, or the sources held no entries
    #[error("no source archives to merge, or they held no entries")]
    Empty,

    /// The output basename is not a usable plugin stem
    #[error("invalid merge basename `{0}`")]
    InvalidBasename(String),

    /// An entry path escaped its archive root
    #[error("unsafe archive path `{0}`")]
    UnsafePath(String),

    /// The game has no carrier plugin format wired for merging
    #[error("merging is not supported for {0}")]
    UnsupportedGame(GameKind),
}

/// Merge `sources` into archives, carriers, and loose STRINGS under `staging`, writing nothing else
pub fn merge(
    sources: &[MergeSource],
    staging: &Utf8Path,
    opts: &MergeOptions,
    game: GameKind,
) -> Result<MergeOutput, MergeError> {
    validate_basename(&opts.basename)?;
    let carrier = carrier_for(game).ok_or(MergeError::UnsupportedGame(game))?;
    let (buckets, conflicts) = collect(sources)?;
    fs::ensure_dir(staging)?;

    let mut archives = Vec::new();
    let mut strings = Vec::new();

    if !buckets.general.is_empty() {
        let carrier = format!("{}_Main", opts.basename);
        let path = staging.join(format!("{carrier} - Main.ba2"));
        let img = ba2::pack_general(&buckets.general, is_sounds_ext)?;
        fs::write_atomic(&path, &img)?;
        drop(img);
        archives.push(MergedArchive {
            archive: path,
            carrier,
            kind: Ba2Kind::General,
        });
    }

    if !buckets.textures.is_empty() {
        let groups = texture_groups(&buckets.textures, opts.texture_group_bytes);
        for (i, range) in groups.iter().enumerate() {
            let nn = format!("{:02}", i + 1);
            let carrier = format!("{}_Textures{nn}", opts.basename);
            let path = staging.join(format!("{carrier} - Textures.ba2"));
            let img = ba2::pack_textures(&buckets.textures[range.clone()])?;
            fs::write_atomic(&path, &img)?;
            drop(img);
            archives.push(MergedArchive {
                archive: path,
                carrier,
                kind: Ba2Kind::Texture,
            });
        }
    }

    let mut carriers = Vec::with_capacity(archives.len());
    for a in &archives {
        let path = staging.join(format!("{}.esl", a.carrier));
        fs::write_atomic(&path, &carrier)?;
        carriers.push(path);
    }

    for (rel, bytes) in &buckets.strings {
        let path = staging.join(rel);
        fs::write_atomic(&path, bytes)?;
        strings.push(path);
    }

    Ok(MergeOutput {
        archives,
        carriers,
        strings,
        conflicts,
    })
}

// ────────────────────────────────────────────────────────────────────────
// Merge core: extract, globally dedupe, bucket
// ────────────────────────────────────────────────────────────────────────

/// The repack buckets a merge routes winning entries into
#[derive(Default)]
struct Buckets {
    /// General files (sounds are stored uncompressed at pack time, not a separate archive)
    general: Vec<Ba2File>,
    /// DX10 textures, path-sorted
    textures: Vec<Ba2Texture>,
    /// Loose STRINGS as (staging-relative safe path, bytes)
    strings: Vec<(Utf8PathBuf, Vec<u8>)>,
}

/// A deduped entry's payload, tagged with what it is so bucketing needs no re-parsing
enum Payload {
    /// A general file's path and bytes
    File { path: String, bytes: Vec<u8> },
    /// A texture's path and reassembled DDS
    Texture { path: String, dds: Vec<u8> },
    /// A STRINGS file's safe staging path and bytes
    Strings { rel: Utf8PathBuf, bytes: Vec<u8> },
}

/// The current winner for a deduped path key
struct Winner {
    /// Override rank of the winning source
    rank: usize,
    /// The winning source archive
    source: Utf8PathBuf,
    /// The kept payload
    payload: Payload,
}

/// Extract every source once, resolve path clashes by rank, and bucket the winners
fn collect(sources: &[MergeSource]) -> Result<(Buckets, Vec<MergeConflict>), MergeError> {
    let mut winners: BTreeMap<String, Winner> = BTreeMap::new();
    let mut conflicts = Vec::new();

    for source in sources {
        let payload = ba2::extract(&source.archive)?;
        for classified in classify(payload)? {
            let Classified {
                key,
                display,
                payload,
            } = classified;

            match winners.entry(key) {
                MapEntry::Vacant(v) => {
                    v.insert(Winner {
                        rank: source.override_rank,
                        source: source.archive.clone(),
                        payload,
                    });
                }
                MapEntry::Occupied(mut o) => {
                    if source.override_rank > o.get().rank {
                        conflicts.push(MergeConflict {
                            path: display,
                            winner: source.archive.clone(),
                            loser: o.get().source.clone(),
                        });
                        o.insert(Winner {
                            rank: source.override_rank,
                            source: source.archive.clone(),
                            payload,
                        });
                    } else {
                        conflicts.push(MergeConflict {
                            path: display,
                            winner: o.get().source.clone(),
                            loser: source.archive.clone(),
                        });
                    }
                }
            }
        }
    }

    if winners.is_empty() {
        return Err(MergeError::Empty);
    }

    // BTreeMap iteration is path-sorted, so buckets and outputs are deterministic
    let mut buckets = Buckets::default();
    for (_key, w) in winners {
        match w.payload {
            Payload::Texture { path, dds } => buckets.textures.push(Ba2Texture { path, dds }),
            Payload::Strings { rel, bytes } => buckets.strings.push((rel, bytes)),
            Payload::File { path, bytes } => buckets.general.push(Ba2File { path, bytes }),
        }
    }
    Ok((buckets, conflicts))
}

/// One extracted entry reduced to its dedupe key, display path, and payload
struct Classified {
    /// Lowercased, forward-slashed dedupe key
    key: String,
    /// Original entry path for conflict reporting
    display: String,
    /// The tagged payload
    payload: Payload,
}

/// Turn a source's payload into classified, keyed entries, validating STRINGS paths
fn classify(payload: ba2::Ba2Payload) -> Result<Vec<Classified>, MergeError> {
    let mut out = Vec::new();
    match payload {
        ba2::Ba2Payload::General(files) => {
            for Ba2File { path, bytes } in files {
                if is_strings_ext(&path) {
                    let rel = safe_string_path(&path)?;
                    out.push(Classified {
                        key: norm_key(rel.as_str()),
                        display: path,
                        payload: Payload::Strings { rel, bytes },
                    });
                } else {
                    out.push(Classified {
                        key: norm_key(&path),
                        display: path.clone(),
                        payload: Payload::File { path, bytes },
                    });
                }
            }
        }
        ba2::Ba2Payload::Textures(textures) => {
            for Ba2Texture { path, dds } in textures {
                out.push(Classified {
                    key: norm_key(&path),
                    display: path.clone(),
                    payload: Payload::Texture { path, dds },
                });
            }
        }
    }
    Ok(out)
}

// ────────────────────────────────────────────────────────────────────────
// Merge policy: naming, texture split, path safety
// ────────────────────────────────────────────────────────────────────────

/// Reject a basename that is not a plain plugin stem
fn validate_basename(basename: &str) -> Result<(), MergeError> {
    let invalid = || MergeError::InvalidBasename(basename.to_owned());
    if basename.is_empty()
        || basename == "."
        || basename == ".."
        || basename.contains(['/', '\\', ':'])
    {
        return Err(invalid());
    }
    let lower = basename.to_ascii_lowercase();
    if lower.ends_with(".esp") || lower.ends_with(".esm") || lower.ends_with(".esl") {
        return Err(invalid());
    }
    Ok(())
}

/// Split path-sorted textures into contiguous groups under `cap`, one greedy group at a time
fn texture_groups(textures: &[Ba2Texture], cap: u64) -> Vec<Range<usize>> {
    let mut groups = Vec::new();
    let mut start = 0usize;
    let mut acc = 0u64;
    for (i, t) in textures.iter().enumerate() {
        let len = t.dds.len() as u64;
        if i > start && acc + len > cap {
            groups.push(start..i);
            start = i;
            acc = 0;
        }
        acc += len;
    }
    if !textures.is_empty() {
        groups.push(start..textures.len());
    }
    groups
}

/// Validate a STRINGS entry path and return its staging-relative path under `Strings/`
fn safe_string_path(entry_path: &str) -> Result<Utf8PathBuf, MergeError> {
    let normalized = entry_path.replace('\\', "/");
    let mut comps = Vec::new();
    for comp in normalized.split('/') {
        if comp.is_empty() || comp == "." || comp == ".." || comp.contains([':', '\0']) {
            return Err(MergeError::UnsafePath(entry_path.to_owned()));
        }
        comps.push(comp);
    }
    let mut rel = Utf8PathBuf::new();
    if !comps[0].eq_ignore_ascii_case("strings") {
        rel.push("Strings");
    }
    for comp in comps {
        rel.push(comp);
    }
    Ok(rel)
}

/// Lowercased, forward-slashed form of an archive path, used only as a dedupe key
fn norm_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

/// The lowercased extension of an archive path, ignoring backslash separators
fn ext_lower(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether a path is a Fallout 4 sound format that should be stored uncompressed
fn is_sounds_ext(path: &str) -> bool {
    matches!(ext_lower(path).as_str(), "wav" | "xwm" | "fuz" | "lip")
}

/// Whether a path is a localized STRINGS file that must ship loose
fn is_strings_ext(path: &str) -> bool {
    matches!(
        ext_lower(path).as_str(),
        "strings" | "dlstrings" | "ilstrings"
    )
}

#[cfg(test)]
#[path = "tests/merge.rs"]
mod tests;
