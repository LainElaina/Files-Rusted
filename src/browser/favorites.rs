use std::{
    collections::HashSet,
    env, fs,
    io::{self, ErrorKind},
    path::PathBuf,
};

const FAVORITES_PATH_ENV: &str = "FILES_RUSTED_FAVORITES_PATH";

pub(super) fn load_favorite_paths() -> Vec<PathBuf> {
    let Some(path) = favorites_storage_path() else {
        return Vec::new();
    };

    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    normalize_favorite_paths(contents.lines().map(PathBuf::from).collect())
}

pub(super) fn save_favorite_paths(paths: &[PathBuf]) -> io::Result<()> {
    let Some(path) = favorites_storage_path() else {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "No favorites storage location is available",
        ));
    };

    let normalized = normalize_favorite_paths(paths.to_vec());
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

pub(super) fn normalize_favorite_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
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

fn favorites_storage_path() -> Option<PathBuf> {
    if let Some(override_path) = env::var_os(FAVORITES_PATH_ENV).map(PathBuf::from) {
        return Some(override_path);
    }

    #[cfg(target_os = "windows")]
    {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Files Rusted").join("favorites.txt"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|root| root.join(".files-rusted").join("favorites.txt"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEST_ENV_LOCK: Mutex<()> = Mutex::new(());

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("files-rusted-{name}-{unique}"))
    }

    #[test]
    fn normalize_favorite_paths_dedupes_and_keeps_only_existing_directories() {
        let dir = test_dir("favorites-normalize");
        let docs = dir.join("docs");
        let file = dir.join("note.txt");
        fs::create_dir_all(&docs).unwrap();
        fs::write(&file, "note").unwrap();

        let normalized =
            normalize_favorite_paths(vec![docs.clone(), docs.clone(), file, dir.join("missing")]);

        assert_eq!(normalized, vec![docs.canonicalize().unwrap()]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_and_load_favorite_paths_round_trip_through_override_path() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let dir = test_dir("favorites-round-trip");
        let docs = dir.join("docs");
        let pics = dir.join("pics");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(&pics).unwrap();

        let storage = dir.join("config").join("favorites.txt");
        unsafe {
            env::set_var(FAVORITES_PATH_ENV, &storage);
        }

        save_favorite_paths(&[docs.clone(), pics.clone()]).unwrap();
        let loaded = load_favorite_paths();

        assert_eq!(
            loaded,
            vec![docs.canonicalize().unwrap(), pics.canonicalize().unwrap()]
        );

        unsafe {
            env::remove_var(FAVORITES_PATH_ENV);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_favorite_paths_returns_empty_when_storage_is_missing() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let dir = test_dir("favorites-missing");
        let storage = dir.join("missing").join("favorites.txt");
        unsafe {
            env::set_var(FAVORITES_PATH_ENV, &storage);
        }

        let loaded = load_favorite_paths();

        assert!(loaded.is_empty());
        unsafe {
            env::remove_var(FAVORITES_PATH_ENV);
        }
    }
}
