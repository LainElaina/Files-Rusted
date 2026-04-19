use std::{
    env, fs,
    io::{self, ErrorKind},
    path::PathBuf,
};

const SETTINGS_PATH_ENV: &str = "FILES_RUSTED_SETTINGS_PATH";
const DEFAULT_SORT_MODE_KEY: &str = "name-asc";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BrowserSettings {
    pub(super) sort_mode_key: String,
    pub(super) show_hidden: bool,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            sort_mode_key: DEFAULT_SORT_MODE_KEY.to_string(),
            show_hidden: false,
        }
    }
}

pub(super) fn load_browser_settings() -> BrowserSettings {
    let Some(path) = settings_storage_path() else {
        return BrowserSettings::default();
    };

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

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let contents = format!(
        "sort_mode={}\nshow_hidden={}\n",
        settings.sort_mode_key, settings.show_hidden
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
            "show_hidden" => {
                settings.show_hidden = matches!(value.trim(), "1" | "true" | "yes" | "on");
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
    fn load_browser_settings_returns_defaults_when_storage_is_missing() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let dir = test_dir("settings-missing");
        let storage = dir.join("missing").join("settings.txt");
        unsafe {
            env::set_var(SETTINGS_PATH_ENV, &storage);
        }

        let settings = load_browser_settings();

        assert_eq!(settings, BrowserSettings::default());
        unsafe {
            env::remove_var(SETTINGS_PATH_ENV);
        }
    }

    #[test]
    fn save_and_load_browser_settings_round_trip_through_override_path() {
        let _lock = TEST_ENV_LOCK.lock().unwrap();
        let dir = test_dir("settings-round-trip");
        let storage = dir.join("config").join("settings.txt");
        unsafe {
            env::set_var(SETTINGS_PATH_ENV, &storage);
        }

        let settings = BrowserSettings {
            sort_mode_key: "modified-newest".to_string(),
            show_hidden: true,
        };
        save_browser_settings(&settings).unwrap();

        let loaded = load_browser_settings();

        assert_eq!(loaded, settings);
        unsafe {
            env::remove_var(SETTINGS_PATH_ENV);
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn parse_browser_settings_ignores_unknown_keys_and_bad_lines() {
        let settings = parse_browser_settings(
            "sort_mode=size-desc\nshow_hidden=yes\nunknown=value\nbad-line\n",
        );

        assert_eq!(
            settings,
            BrowserSettings {
                sort_mode_key: "size-desc".to_string(),
                show_hidden: true,
            }
        );
    }
}
