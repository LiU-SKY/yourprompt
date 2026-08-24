//! SWE-bench Lite: is the grounding axis actually reading the repository?
//!
//! This is the test that matters. Axis A is the whole reason this tool is not
//! another prompt scorer, and no text-only dataset can exercise it, because a
//! prompt with no repository has nothing to be grounded in.
//!
//! SWE-bench Lite supplies what is needed: 300 real GitHub issues, each paired
//! with the repository it was filed against and the patch that actually fixed
//! it. Two experiments follow from that.
//!
//! # The cross-repository control
//!
//! Score each issue against its own repository, then against unrelated ones.
//! Axis A must score it higher against the codebase it actually belongs to. If
//! swapping the repository does not move the number, the axis is measuring
//! nothing repository-specific and the central claim of this project is false.
//!
//! This needs no labels and cannot be satisfied by accident: a scorer that
//! ignores the corpus produces an identical number both times, and one that
//! merely likes long prompts produces the same number both times too. Only
//! genuine resolution against the repository's vocabulary can separate them.
//!
//! # The gold-patch test
//!
//! The patch names the files that actually had to change. An issue that names
//! one of them has told the agent where to go; one that does not has left it
//! to search. Resolution scores should separate those two groups.
//!
//! # One index per repository
//!
//! Instances of the same repository sit at different base commits, but a
//! repository's vocabulary barely moves between nearby commits, so a single
//! index per repository is built at the base commit of its first instance.
//! That turns three hundred downloads into twelve. It is an approximation and
//! is named as one.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use super::{cache_dir, download, Tally};
use crate::repo::IndexCorpus;

/// Rows come from the Hugging Face datasets server, which serves JSON and so
/// needs no Parquet reader.
const ROWS_URL: &str = "https://datasets-server.huggingface.co/rows\
?dataset=princeton-nlp%2FSWE-bench_Lite&config=default&split=test";

const TOTAL_ROWS: usize = 300;
const PAGE: usize = 100;

/// One SWE-bench instance, reduced to what this benchmark reads.
struct Instance {
    repo: String,
    base_commit: String,
    problem_statement: String,
    /// Files the gold patch touches, as repository-relative paths.
    changed_files: Vec<String>,
}

/// Files named by a unified diff.
fn changed_files(patch: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in patch.lines() {
        let Some(rest) = line.strip_prefix("diff --git a/") else {
            continue;
        };
        let Some((path, _)) = rest.split_once(" b/") else {
            continue;
        };
        if !files.iter().any(|f| f == path) {
            files.push(path.to_string());
        }
    }
    files
}

/// Does the issue name one of the files the fix actually touched?
///
/// Either the full repository-relative path or the bare file name counts. A
/// bare name is weaker evidence, but it is still the user pointing somewhere.
fn names_a_changed_file(problem: &str, changed: &[String]) -> bool {
    let lower = problem.to_lowercase();
    changed.iter().any(|path| {
        let path = path.to_lowercase();
        if lower.contains(&path) {
            return true;
        }
        path.rsplit('/')
            .next()
            .is_some_and(|base| base.len() > 4 && lower.contains(base))
    })
}

fn dataset_path() -> Option<PathBuf> {
    cache_dir().map(|d| d.join("swe-bench-lite.jsonl"))
}

/// Fetch all instances, paging through the rows endpoint, and cache as JSONL.
fn ensure_dataset(refresh: bool) -> Result<Vec<Instance>, String> {
    let path = dataset_path().ok_or("no state directory available")?;

    if !path.exists() || refresh {
        eprintln!("fetching SWE-bench Lite...");
        let mut combined = String::new();
        for offset in (0..TOTAL_ROWS).step_by(PAGE) {
            let page = path.with_extension(format!("page{offset}"));
            let url = format!("{ROWS_URL}&offset={offset}&length={PAGE}");
            download(&url, &page, true)?;
            let text = std::fs::read_to_string(&page).map_err(|e| e.to_string())?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| format!("offset {offset}: {e}"))?;
            for row in value["rows"].as_array().unwrap_or(&Vec::new()) {
                combined.push_str(&row["row"].to_string());
                combined.push('\n');
            }
            let _ = std::fs::remove_file(&page);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, combined).map_err(|e| e.to_string())?;
    }

    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut instances = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let row: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let get = |key: &str| row[key].as_str().unwrap_or_default().to_string();
        let problem_statement = get("problem_statement");
        if problem_statement.trim().is_empty() {
            continue;
        }
        instances.push(Instance {
            repo: get("repo"),
            base_commit: get("base_commit"),
            changed_files: changed_files(&get("patch")),
            problem_statement,
        });
    }
    Ok(instances)
}

fn index_path(repo: &str) -> Option<PathBuf> {
    cache_dir().map(|d| {
        d.join("index")
            .join(format!("{}.tsv", repo.replace('/', "__")))
    })
}

/// Download a repository at `sha`, index it, and throw the source away.
///
/// Only the index is kept. A dozen checked-out repositories would be several
/// gigabytes; a dozen indexes are a few hundred kilobytes, and the index is
/// all the grounding axis ever reads.
fn ensure_index(repo: &str, sha: &str, refresh: bool) -> Result<PathBuf, String> {
    let path = index_path(repo).ok_or("no state directory available")?;
    if path.exists() && !refresh {
        return Ok(path);
    }

    let cache = cache_dir().ok_or("no state directory available")?;
    let work = cache.join("work");
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;

    let tarball = work.join("repo.tar.gz");
    let url = format!("https://codeload.github.com/{repo}/tar.gz/{sha}");
    eprintln!("  fetching {repo} at {}...", &sha[..sha.len().min(8)]);
    download(&url, &tarball, true)?;

    // A non-zero exit is not necessarily fatal. Some archives carry paths the
    // host filesystem cannot represent -- django ships a test fixture literally
    // named "⊗.txt", which Windows refuses -- and tar extracts everything else
    // before reporting the failure at the end. Losing a whole repository over
    // one test fixture would silently drop the largest slice of the dataset,
    // so the extraction is judged by what actually landed on disk.
    let status = Command::new("tar")
        .arg("-xzf")
        .arg(&tarball)
        .arg("-C")
        .arg(&work)
        .status()
        .map_err(|e| format!("could not run tar: {e}"))?;
    let _ = std::fs::remove_file(&tarball);

    // The archive unpacks into a single `name-sha` directory.
    let root = std::fs::read_dir(&work)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.is_dir())
        .ok_or_else(|| {
            format!(
                "{repo}: tar produced no directory (exit {:?})",
                status.code()
            )
        })?;
    if !status.success() {
        eprintln!("    (tar reported errors; continuing with what it extracted)");
    }

    let index = yp_index::RepoIndex::build(&root);
    index.save(&path).map_err(|e| e.to_string())?;
    eprintln!(
        "  indexed {repo}: {} files, {} terms",
        index.files(),
        index.distinct_terms()
    );

    let _ = std::fs::remove_dir_all(&work);
    Ok(path)
}

/// How one issue scored against its own repository versus the others.
struct Trial {
    repo: String,
    own: f64,
    foreign_mean: f64,
    foreign_best: f64,
    own_resolution: f64,
    names_changed_file: bool,
}

fn resolution_of(score: &yp_core::Score) -> f64 {
    score
        .grounding
        .as_ref()
        .and_then(|axis| axis.components.iter().find(|c| c.id == "resolution"))
        .map(|c| c.earned)
        .unwrap_or(0.0)
}

fn render(trials: &[Trial], repos: &[String]) -> String {
    let mut out = String::new();
    out.push_str("# yourprompt benchmark -- grounding\n\n");
    out.push_str(&format!(
        "Dataset: [SWE-bench Lite](https://huggingface.co/datasets/princeton-nlp/SWE-bench_Lite), \
         {} real GitHub issues across {} repositories.\n\n\
         Axis A claims to read your repository. This checks that claim the only \
         way that cannot be satisfied by accident: score each issue against the \
         codebase it was actually filed against, then against unrelated ones. A \
         scorer that ignores the corpus returns the same number both times. So \
         does one that merely rewards long prompts. Only genuine resolution \
         against a repository's vocabulary can tell them apart.\n\n",
        trials.len(),
        repos.len(),
    ));

    // ---- cross-repository control ---------------------------------------
    let mut vs_mean = Tally::default();
    let mut vs_best = Tally::default();
    for trial in trials {
        vs_mean.add(trial.own, trial.foreign_mean);
        vs_best.add(trial.own, trial.foreign_best);
    }

    out.push_str(&format!(
        "## Cross-repository control\n\n\
         | Comparison | Issues | Own repository scores higher | Mean margin |\n\
         |---|---:|---:|---:|\n\
         | versus the mean foreign repository | {} | **{:.1}%** | {:.1} |\n\
         | versus the *best* foreign repository | {} | **{:.1}%** | {:.1} |\n\n",
        vs_mean.total,
        vs_mean.accuracy() * 100.0,
        vs_mean.mean_delta(),
        vs_best.total,
        vs_best.accuracy() * 100.0,
        vs_best.mean_delta(),
    ));

    // Per repository, so a single dominant repository cannot carry the result.
    let mut by_repo: BTreeMap<&str, Tally> = BTreeMap::new();
    for trial in trials {
        by_repo
            .entry(trial.repo.as_str())
            .or_default()
            .add(trial.own, trial.foreign_mean);
    }
    out.push_str(
        "| Repository | Issues | Own scores higher | Mean margin |\n|---|---:|---:|---:|\n",
    );
    for (repo, tally) in &by_repo {
        out.push_str(&format!(
            "| {} | {} | {:.1}% | {:.1} |\n",
            repo,
            tally.total,
            tally.accuracy() * 100.0,
            tally.mean_delta(),
        ));
    }

    // ---- gold-patch test -------------------------------------------------
    let (named, unnamed): (Vec<&Trial>, Vec<&Trial>) =
        trials.iter().partition(|t| t.names_changed_file);
    let mean = |group: &[&Trial]| -> f64 {
        if group.is_empty() {
            return 0.0;
        }
        group.iter().map(|t| t.own_resolution).sum::<f64>() / group.len() as f64
    };

    out.push_str(&format!(
        "\n## Gold-patch test\n\n\
         The patch names the files that actually had to change. An issue naming \
         one of them has told the agent where to go; one that does not has left \
         it to search. Mean `resolution` score, out of 150:\n\n\
         | Issue names a file the fix touched | Issues | Mean resolution |\n\
         |---|---:|---:|\n\
         | yes | {} | **{:.1}** |\n\
         | no | {} | {:.1} |\n\n\
         Separation: **{:+.1}** points.\n",
        named.len(),
        mean(&named),
        unnamed.len(),
        mean(&unnamed),
        mean(&named) - mean(&unnamed),
    ));

    out.push_str(
        "\n## Caveats\n\n\
         - One index per repository, built at the base commit of that \
         repository's first instance. Repositories evolve, but their \
         vocabulary barely moves between nearby commits, and this turns three \
         hundred downloads into a dozen. It is an approximation.\n\
         - SWE-bench issues are written by maintainers and users to be filed, \
         not typed at a coding agent. They are far longer than a typical \
         prompt.\n\
         - The cross-repository control shows the axis is repository-specific. \
         It does not show that the resulting *number* predicts whether an \
         agent will succeed; that would need agent runs, not scores.\n",
    );

    out
}

pub fn run(
    repos_wanted: usize,
    limit: Option<usize>,
    refresh: bool,
    report: Option<String>,
) -> ExitCode {
    let instances = match ensure_dataset(refresh) {
        Ok(instances) => instances,
        Err(e) => {
            eprintln!("yp: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Prefer the repositories with the most instances, so the comparison rests
    // on as many issues as possible for a fixed number of downloads.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for instance in &instances {
        *counts.entry(instance.repo.as_str()).or_default() += 1;
    }
    let mut ranked: Vec<(&str, usize)> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let selected: Vec<String> = ranked
        .into_iter()
        .take(repos_wanted.max(2))
        .map(|(repo, _)| repo.to_string())
        .collect();
    let selected_set: HashSet<&str> = selected.iter().map(String::as_str).collect();

    eprintln!("preparing {} repositories...", selected.len());
    let mut corpora: BTreeMap<String, IndexCorpus> = BTreeMap::new();
    for repo in &selected {
        let Some(instance) = instances.iter().find(|i| &i.repo == repo) else {
            continue;
        };
        match ensure_index(repo, &instance.base_commit, refresh) {
            Ok(path) => match crate::repo::load_index_at(&path) {
                Some(corpus) => {
                    corpora.insert(repo.clone(), corpus);
                }
                None => eprintln!("  skipping {repo}: index unreadable"),
            },
            Err(e) => eprintln!("  skipping {repo}: {e}"),
        }
    }
    if corpora.len() < 2 {
        eprintln!("yp: need at least two indexed repositories to compare");
        return ExitCode::FAILURE;
    }

    let mut trials = Vec::new();
    for instance in instances
        .iter()
        .filter(|i| selected_set.contains(i.repo.as_str()) && corpora.contains_key(&i.repo))
        .take(limit.unwrap_or(usize::MAX))
    {
        let Some(own_corpus) = corpora.get(&instance.repo) else {
            continue;
        };
        let Some(own) = yp_core::score_with(
            &instance.problem_statement,
            Some(own_corpus as &dyn yp_core::Corpus),
        ) else {
            continue;
        };

        let mut foreign = Vec::new();
        for (repo, corpus) in &corpora {
            if repo == &instance.repo {
                continue;
            }
            if let Some(score) = yp_core::score_with(
                &instance.problem_statement,
                Some(corpus as &dyn yp_core::Corpus),
            ) {
                foreign.push(score.total);
            }
        }
        if foreign.is_empty() {
            continue;
        }
        let foreign_mean = foreign.iter().sum::<f64>() / foreign.len() as f64;
        let foreign_best = foreign.iter().copied().fold(f64::MIN, f64::max);

        trials.push(Trial {
            repo: instance.repo.clone(),
            own: own.total,
            foreign_mean,
            foreign_best,
            own_resolution: resolution_of(&own),
            names_changed_file: names_a_changed_file(
                &instance.problem_statement,
                &instance.changed_files,
            ),
        });
    }

    if trials.is_empty() {
        eprintln!("yp: no comparable issues");
        return ExitCode::FAILURE;
    }

    let indexed: Vec<String> = corpora.keys().cloned().collect();
    let rendered = render(&trials, &indexed);
    match report {
        Some(path) => {
            if let Err(e) = std::fs::write(&path, &rendered) {
                eprintln!("yp: could not write {path}: {e}");
                return ExitCode::FAILURE;
            }
            println!("wrote {path}");
        }
        None => print!("{rendered}"),
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_files_a_patch_touches() {
        let patch = "diff --git a/astropy/modeling/separable.py b/astropy/modeling/separable.py\n\
                     --- a/astropy/modeling/separable.py\n\
                     +++ b/astropy/modeling/separable.py\n\
                     @@ -242,7 +242,7 @@\n\
                     diff --git a/docs/changes/1.rst b/docs/changes/1.rst\n";
        assert_eq!(
            changed_files(patch),
            ["astropy/modeling/separable.py", "docs/changes/1.rst"]
        );
    }

    #[test]
    fn a_patch_with_no_diff_headers_names_no_files() {
        assert!(changed_files("").is_empty());
        assert!(changed_files("just some prose").is_empty());
    }

    #[test]
    fn a_repeated_file_is_listed_once() {
        let patch = "diff --git a/a.py b/a.py\ndiff --git a/a.py b/a.py\n";
        assert_eq!(changed_files(patch), ["a.py"]);
    }

    #[test]
    fn recognises_a_full_path_named_in_the_issue() {
        let changed = vec!["astropy/modeling/separable.py".to_string()];
        assert!(names_a_changed_file(
            "the bug is in astropy/modeling/separable.py near the top",
            &changed
        ));
    }

    #[test]
    fn recognises_a_bare_file_name_named_in_the_issue() {
        let changed = vec!["astropy/modeling/separable.py".to_string()];
        assert!(names_a_changed_file("separable.py is wrong", &changed));
    }

    #[test]
    fn does_not_match_a_short_file_name_by_accident() {
        // "a.py" is too short to be evidence -- it would fire on prose.
        let changed = vec!["src/a.py".to_string()];
        assert!(!names_a_changed_file(
            "something about a python thing",
            &changed
        ));
    }

    #[test]
    fn a_windows_style_diff_path_is_still_read() {
        // Patches always use forward slashes regardless of platform.
        let patch = "diff --git a/src/sub dir/file.py b/src/sub dir/file.py
";
        assert_eq!(changed_files(patch), ["src/sub dir/file.py"]);
    }

    #[test]
    fn a_malformed_diff_header_is_skipped_not_panicked_on() {
        assert!(changed_files(
            "diff --git a/only-one-side
"
        )
        .is_empty());
        assert!(changed_files(
            "diff --git 
"
        )
        .is_empty());
    }

    #[test]
    fn file_name_matching_ignores_case() {
        let changed = vec!["astropy/modeling/Separable.py".to_string()];
        assert!(names_a_changed_file("SEPARABLE.PY is wrong", &changed));
    }

    #[test]
    fn no_changed_files_means_nothing_can_be_named() {
        assert!(!names_a_changed_file("anything at all", &[]));
    }

    #[test]
    fn an_issue_naming_nothing_is_reported_as_such() {
        let changed = vec!["astropy/modeling/separable.py".to_string()];
        assert!(!names_a_changed_file(
            "the output is wrong when models are nested",
            &changed
        ));
    }
}
