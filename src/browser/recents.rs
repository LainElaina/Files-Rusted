use std::{
    collections::HashSet,
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

const RECENTS_PATH_ENV: &str = "FILES_RUSTED_RECENTS_PATH";
const MAX_RECENT_PATHS: usize = 8;

pub(super) fn load_recent_paths() -> Vec<PathBuf> {
    let Some(path) = recents_storage_path() else {
        return Vec::new();
    };

    load_recent_paths_from_path(&path)
}

fn load_recent_paths_from_path(path: &Path) -> Vec<PathBuf> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    normalize_recent_paths(contents.lines().map(PathBuf::from).collect())
}

pub(super) fn save_recent_paths(paths: &[PathBuf]) -> io::Result<()> {
    let Some(path) = recents_storage_path() else {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "No recents storage location is available",
        ));
    };

    save_recent_paths_to_path(&path, paths)
}

fn save_recent_paths_to_path(path: &Path, paths: &[PathBuf]) -> io::Result<()> {
    let normalized = normalize_recent_paths(paths.to_vec());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut contents = normalized
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    if !contents.is_empty() {
        contents.push('\n');
    }

    fs::write(path, contents)
}

pub(super) fn remember_recent_path(existing_paths: &[PathBuf], path: &Path) -> Vec<PathBuf> {
    if !path.is_dir() {
        return normalize_recent_paths(existing_paths.to_vec());
    }

    let candidate = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut next = vec![candidate];
    next.extend(existing_paths.iter().cloned());
    let mut normalized = normalize_recent_paths(next);
    normalized.truncate(MAX_RECENT_PATHS);
    normalized
}

pub(super) fn normalize_recent_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();

    for path in paths {
        if !path.is_dir() {
            continue;
        }

        let canonical = path.canonicalize().unwrap_or(path);
        if seen.insert(canonical.clone()) {
            normalized.push(canonical);
        }
    }

    normalized
}

fn recents_storage_path() -> Option<PathBuf> {
    if let Some(override_path) = env::var_os(RECENTS_PATH_ENV).map(PathBuf::from) {
        return Some(override_path);
    }

    #[cfg(target_os = "windows")]
    {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Files Rusted").join("recent-directories.txt"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|root| root.join(".files-rusted").join("recent-directories.txt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("files-rusted-{name}-{unique}"))
    }

    #[test]
    fn remember_recent_path_promotes_newest_directory_to_front() {
        let dir = test_dir("recents-promote");
        let docs = dir.join("docs");
        let pics = dir.join("pics");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&pics).unwrap();

        let recents = remember_recent_path(&[docs.clone()], &pics);

        assert_eq!(
            recents,
            vec![pics.canonicalize().unwrap(), docs.canonicalize().unwrap()]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_and_load_recent_paths_round_trip_through_override_path() {
        let dir = test_dir("recents-round-trip");
        let docs = dir.join("docs");
        let pics = dir.join("pics");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&pics).unwrap();

        let storage = dir.join("config").join("recent-directories.txt");

        save_recent_paths_to_path(&storage, &[docs.clone(), pics.clone()]).unwrap();
        let loaded = load_recent_paths_from_path(&storage);

        assert_eq!(
            loaded,
            vec![docs.canonicalize().unwrap(), pics.canonicalize().unwrap()]
        );

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_recent_paths_returns_empty_when_storage_is_missing() {
        let dir = test_dir("recents-missing");
        let storage = dir.join("missing").join("recent-directories.txt");

        let loaded = load_recent_paths_from_path(&storage);

        assert!(loaded.is_empty());
    }
}
