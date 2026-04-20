use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

const SETTINGS_PATH_ENV: &str = "FILES_RUSTED_SETTINGS_PATH";
const DEFAULT_SORT_MODE_KEY: &str = "name-asc";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BrowserSettings {
    pub(super) sort_mode_key: String,
    pub(super) conflict_strategy_key: String,
    pub(super) show_hidden: bool,
    pub(super) last_directory: Option<PathBuf>,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            sort_mode_key: DEFAULT_SORT_MODE_KEY.to_string(),
            conflict_strategy_key: "keep-both".to_string(),
            show_hidden: false,
            last_directory: None,
        }
    }
}

pub(super) fn load_browser_settings() -> BrowserSettings {
    let Some(path) = settings_storage_path() else {
        return BrowserSettings::default();
    };

    load_browser_settings_from_path(&path)
}

fn load_browser_settings_from_path(path: &Path) -> BrowserSettings {
    let Ok(contents) = fs::read_to_string(path) else {
        return BrowserSettings::default();
    };

    parse_browser_settings(&contents)
}

pub(super) fn save_browser_settings(settings: &BrowserSettings) -> io::Result<()> {
    let Some(path) = settings_storage_path() else {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "No settings storage location is available",
        ));
    };

    save_browser_settings_to_path(&path, settings)
}

fn save_browser_settings_to_path(path: &Path, settings: &BrowserSettings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = format!(
        "sort_mode={}\nconflict_strategy={}\nshow_hidden={}\nlast_directory={}\n",
        settings.sort_mode_key,
        settings.conflict_strategy_key,
        settings.show_hidden,
        settings
            .last_directory
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    fs::write(path, contents)
}

fn parse_browser_settings(contents: &str) -> BrowserSettings {
    let mut settings = BrowserSettings::default();

    for line in contents.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        match key.trim() {
            "sort_mode" => {
                let value = value.trim();
                if !value.is_empty() {
                    settings.sort_mode_key = value.to_string();
                }
            }
            "conflict_strategy" => {
                let value = value.trim();
                if !value.is_empty() {
                    settings.conflict_strategy_key = value.to_string();
                }
            }
            "show_hidden" => {
                settings.show_hidden = matches!(value.trim(), "1" | "true" | "yes" | "on");
            }
            "last_directory" => {
                let value = value.trim();
                if !value.is_empty() {
                    settings.last_directory = Some(PathBuf::from(value));
                }
            }
            _ => {}
        }
    }

    settings
}

fn settings_storage_path() -> Option<PathBuf> {
    if let Some(override_path) = env::var_os(SETTINGS_PATH_ENV).map(PathBuf::from) {
        return Some(override_path);
    }

    #[cfg(target_os = "windows")]
    {
        return env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Files Rusted").join("settings.txt"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        env::var_os("HOME")
            .map(PathBuf::from)
            .map(|root| root.join(".files-rusted").join("settings.txt"))
    }
}

pub(super) fn resolve_start_directory(
    fallback_dir: &Path,
    last_directory: Option<&PathBuf>,
) -> PathBuf {
    last_directory
        .filter(|path| path.is_dir())
        .cloned()
        .unwrap_or_else(|| fallback_dir.to_path_buf())
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
    fn load_browser_settings_returns_defaults_when_storage_is_missing() {
        let dir = test_dir("settings-missing");
        let storage = dir.join("missing").join("settings.txt");

        let settings = load_browser_settings_from_path(&storage);

        assert_eq!(settings, BrowserSettings::default());
    }

    #[test]
    fn save_and_load_browser_settings_round_trip_through_override_path() {
        let dir = test_dir("settings-round-trip");
        let storage = dir.join("config").join("settings.txt");

        let settings = BrowserSettings {
            sort_mode_key: "modified-newest".to_string(),
            conflict_strategy_key: "overwrite".to_string(),
            show_hidden: true,
            last_directory: Some(dir.join("workspace")),
        };
        fs::create_dir_all(settings.last_directory.as_ref().unwrap()).unwrap();
        save_browser_settings_to_path(&storage, &settings).unwrap();

        let loaded = load_browser_settings_from_path(&storage);

        assert_eq!(loaded, settings);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_browser_settings_ignores_unknown_keys_and_bad_lines() {
        let settings = parse_browser_settings(
            "sort_mode=size-desc\nconflict_strategy=skip\nshow_hidden=yes\nunknown=value\nbad-line\n",
        );

        assert_eq!(
            settings,
            BrowserSettings {
                sort_mode_key: "size-desc".to_string(),
                conflict_strategy_key: "skip".to_string(),
                show_hidden: true,
                last_directory: None,
            }
        );
    }

    #[test]
    fn resolve_start_directory_prefers_existing_last_directory() {
        let dir = test_dir("settings-last-dir");
        let fallback = dir.join("fallback");
        let remembered = dir.join("remembered");
        fs::create_dir_all(&fallback).unwrap();
        fs::create_dir_all(&remembered).unwrap();

        let resolved = resolve_start_directory(&fallback, Some(&remembered));

        assert_eq!(resolved, remembered);
        fs::remove_dir_all(&dir).unwrap();
    }
}
