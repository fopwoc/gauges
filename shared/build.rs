use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo::rerun-if-env-changed=BUILD_VERSION");

    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    emit_git_rerun_paths(&repository);

    let version = env::var("BUILD_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| git_version(&repository));
    if !is_release_tag(&version) && !is_development_version(&version) {
        eprintln!(
            "BUILD_VERSION must be a semantic release tag or YYYYMMDD-short-hash development version"
        );
        std::process::exit(1);
    }
    println!("cargo::rustc-env=BUILD_VERSION={version}");
}

fn git_version(repository: &Path) -> String {
    if is_clean(repository)
        && let Some(tag) = git(repository, &["describe", "--tags", "--exact-match", "HEAD"])
        && is_release_tag(&tag)
    {
        return tag;
    }

    let date = git(repository, &["show", "-s", "--format=%cs", "HEAD"])
        .map(|value| value.replace('-', ""));
    let commit = git(repository, &["rev-parse", "--short=8", "HEAD"]);
    match (date, commit) {
        (Some(date), Some(commit)) => format!("{date}-{commit}"),
        _ => {
            eprintln!("Git metadata is unavailable; set BUILD_VERSION explicitly");
            std::process::exit(1);
        }
    }
}

fn is_clean(repository: &Path) -> bool {
    git(
        repository,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .is_some_and(|output| output.is_empty())
}

fn is_release_tag(value: &str) -> bool {
    let mut parts = value.split('.');
    parts
        .by_ref()
        .take(3)
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        && parts.next().is_none()
        && value.matches('.').count() == 2
}

fn is_development_version(value: &str) -> bool {
    let Some((date, commit)) = value.split_once('-') else {
        return false;
    };
    date.len() == 8
        && date.bytes().all(|byte| byte.is_ascii_digit())
        && commit.len() == 8
        && commit.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn git(repository: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn emit_git_rerun_paths(repository: &Path) {
    let git_dir = repository.join(".git");
    println!("cargo::rerun-if-changed={}", git_dir.join("HEAD").display());
    println!(
        "cargo::rerun-if-changed={}",
        git_dir.join("packed-refs").display()
    );

    let Ok(head) = std::fs::read_to_string(git_dir.join("HEAD")) else {
        return;
    };
    let Some(reference) = head.trim().strip_prefix("ref: ") else {
        return;
    };
    let reference = PathBuf::from(reference);
    println!(
        "cargo::rerun-if-changed={}",
        git_dir.join(reference).display()
    );
}
