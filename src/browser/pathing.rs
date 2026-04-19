use super::{DirectoryEntry, SidebarEntry};
use crate::BreadcrumbEntry;
use chrono::{DateTime, Local};
use jwalk::WalkDir;
use slint::SharedString;
use std::{
    env,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
#[cfg(test)]
use std::{fs, time::SystemTime};

pub(super) fn build_sidebar_entries(
    start_dir: &Path,
    favorite_paths: &[PathBuf],
) -> (Vec<SidebarEntry>, Vec<PathBuf>) {
    let mut entries = Vec::new();
    let mut paths = Vec::new();

    if let Some(home) = home_directory() {
        push_sidebar_entry(&mut entries, &mut paths, "Home", home);
    }

    push_sidebar_entry(
        &mut entries,
        &mut paths,
        "Workspace",
        start_dir.to_path_buf(),
    );
    push_sidebar_entry(&mut entries, &mut paths, "Root", filesystem_root(start_dir));

    for favorite in favorite_paths {
        push_sidebar_entry_with_caption(
            &mut entries,
            &mut paths,
            &favorite_label(favorite),
            &short_path_label(favorite),
            favorite.clone(),
        );
    }

    (entries, paths)
}

pub(super) fn build_breadcrumbs(current_dir: &Path) -> (Vec<BreadcrumbEntry>, Vec<PathBuf>) {
    let mut paths = current_dir
        .ancestors()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    paths.reverse();

    let items = paths
        .iter()
        .map(|path| BreadcrumbEntry {
            label: SharedString::from(breadcrumb_label(path)),
        })
        .collect::<Vec<_>>();

    (items, paths)
}

pub(super) fn load_directory_entries(path: &Path) -> Result<Vec<DirectoryEntry>, std::io::Error> {
    let entries = WalkDir::new(path)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() == 1)
        .filter_map(|entry| {
            let path = entry.path();
            let metadata = entry.metadata().ok()?;
            let name = entry.file_name().to_string_lossy().into_owned();

            if name.is_empty() {
                return None;
            }

            let is_dir = metadata.is_dir();
            let is_hidden = hidden_file_flag(&name, &metadata);
            let size_bytes = if is_dir { 0 } else { metadata.len() };
            let modified_timestamp = metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs());

            Some(DirectoryEntry {
                path_label: path.display().to_string(),
                kind_label: if is_dir {
                    "Folder".to_string()
                } else {
                    file_kind_label(&path)
                },
                size_label: if is_dir {
                    "—".to_string()
                } else {
                    format_bytes(size_bytes)
                },
                name_lower: name.to_lowercase(),
                path,
                name,
                is_dir,
                is_hidden,
                size_bytes,
                modified_label: modified_timestamp
                    .map(format_modified_timestamp)
                    .unwrap_or_else(|| "—".to_string()),
                modified_timestamp,
            })
        })
        .collect::<Vec<_>>();

    Ok(entries)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PathNavigationTarget {
    Directory(PathBuf),
    File { path: PathBuf, parent_dir: PathBuf },
}

pub(super) fn resolve_navigation_target(
    input: &str,
    current_dir: &Path,
) -> Result<PathNavigationTarget, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Path cannot be empty".to_string());
    }

    let raw_path = PathBuf::from(trimmed);
    let resolved = if raw_path.is_absolute() {
        raw_path
    } else {
        current_dir.join(raw_path)
    };

    if resolved.is_dir() {
        return Ok(PathNavigationTarget::Directory(resolved));
    }

    if resolved.is_file() {
        let Some(parent_dir) = resolved.parent().map(Path::to_path_buf) else {
            return Err("Cannot navigate to a file without a parent directory".to_string());
        };

        return Ok(PathNavigationTarget::File {
            path: resolved,
            parent_dir,
        });
    }

    Err(format!("Path not found: {}", trimmed))
}

pub(super) fn current_sidebar_index(sidebar_paths: &[PathBuf], current_dir: &Path) -> i32 {
    sidebar_paths
        .iter()
        .enumerate()
        .filter(|(_, path)| current_dir.starts_with(path))
        .max_by_key(|(_, path)| path.components().count())
        .map(|(index, _)| index as i32)
        .unwrap_or(0)
}

pub(super) fn short_path_label(path: &Path) -> String {
    let text = path.display().to_string();
    let chars = text.chars().collect::<Vec<_>>();

    if chars.len() <= 30 {
        text
    } else {
        let suffix = chars[chars.len() - 29..].iter().collect::<String>();
        format!("…{suffix}")
    }
}

fn push_sidebar_entry(
    entries: &mut Vec<SidebarEntry>,
    paths: &mut Vec<PathBuf>,
    label: &str,
    path: PathBuf,
) {
    push_sidebar_entry_with_caption(entries, paths, label, &short_path_label(&path), path);
}

fn push_sidebar_entry_with_caption(
    entries: &mut Vec<SidebarEntry>,
    paths: &mut Vec<PathBuf>,
    label: &str,
    caption: &str,
    path: PathBuf,
) {
    if paths.iter().any(|existing| existing == &path) {
        return;
    }

    entries.push(SidebarEntry {
        label: SharedString::from(label),
        caption: SharedString::from(caption),
    });
    paths.push(path);
}

fn favorite_label(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| path.display().to_string());
    format!("★ {name}")
}

fn breadcrumb_label(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            let text = path.display().to_string();
            if text.is_empty() {
                "/".to_string()
            } else {
                text
            }
        })
}

fn home_directory() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("USERPROFILE").map(PathBuf::from))
}

fn filesystem_root(path: &Path) -> PathBuf {
    let mut root = path.to_path_buf();

    while let Some(parent) = root.parent() {
        root = parent.to_path_buf();
    }

    root
}

fn file_kind_label(path: &Path) -> String {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty());

    match extension {
        Some(extension) => format!("{} file", extension.to_uppercase()),
        None => "File".to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

fn format_modified_timestamp(timestamp: u64) -> String {
    let datetime = DateTime::<Local>::from(UNIX_EPOCH + std::time::Duration::from_secs(timestamp));
    datetime.format("%Y-%m-%d %H:%M").to_string()
}

#[cfg(target_os = "windows")]
fn hidden_file_flag(name: &str, metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

    name.starts_with('.') || metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
}

#[cfg(not(target_os = "windows"))]
fn hidden_file_flag(name: &str, _metadata: &std::fs::Metadata) -> bool {
    name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_sidebar_index_prefers_deepest_matching_path() {
        let sidebar_paths = vec![
            PathBuf::from("/"),
            PathBuf::from("/workspace"),
            PathBuf::from("/workspace/project"),
        ];

        assert_eq!(
            current_sidebar_index(&sidebar_paths, Path::new("/workspace/project/src")),
            2
        );
        assert_eq!(
            current_sidebar_index(&sidebar_paths, Path::new("/workspace/other")),
            1
        );
    }

    #[test]
    fn build_breadcrumbs_preserves_path_order_and_labels() {
        let (items, paths) = build_breadcrumbs(Path::new("/workspace/project/src"));

        assert_eq!(
            paths,
            vec![
                PathBuf::from("/"),
                PathBuf::from("/workspace"),
                PathBuf::from("/workspace/project"),
                PathBuf::from("/workspace/project/src"),
            ]
        );
        assert_eq!(items[0].label.as_str(), "/");
        assert_eq!(items[1].label.as_str(), "workspace");
        assert_eq!(items[2].label.as_str(), "project");
        assert_eq!(items[3].label.as_str(), "src");
    }

    #[test]
    fn load_directory_entries_reads_only_depth_one_children() {
        let dir = test_dir("depth-one-jwalk");
        fs::create_dir_all(dir.join("nested/inner")).unwrap();
        fs::write(dir.join("root.txt"), "root").unwrap();
        fs::write(dir.join("nested/child.txt"), "child").unwrap();

        let mut names = load_directory_entries(&dir)
            .unwrap()
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        names.sort();

        assert_eq!(names, vec!["nested".to_string(), "root.txt".to_string()]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hidden_file_flag_marks_dotfiles_as_hidden() {
        let dir = test_dir("hidden-dotfile");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join(".secret.txt");
        fs::write(&file, "secret").unwrap();

        let metadata = fs::metadata(&file).unwrap();

        assert!(hidden_file_flag(".secret.txt", &metadata));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn build_sidebar_entries_appends_favorites_after_default_locations() {
        let start_dir = PathBuf::from("/workspace/project");
        let favorites = vec![PathBuf::from("/workspace/project/docs")];

        let (entries, paths) = build_sidebar_entries(&start_dir, &favorites);

        assert!(entries.iter().any(|entry| entry.label.as_str() == "Home"));
        assert!(entries
            .iter()
            .any(|entry| entry.label.as_str() == "Workspace"));
        assert!(entries.iter().any(|entry| entry.label.as_str() == "Root"));
        assert!(entries.iter().any(|entry| entry.label.as_str() == "★ docs"));
        assert!(paths.contains(&PathBuf::from("/workspace/project/docs")));
    }

    #[test]
    fn resolve_navigation_target_resolves_relative_directory_against_current_dir() {
        let dir = test_dir("resolve-dir");
        let docs = dir.join("docs");
        fs::create_dir_all(&docs).unwrap();

        let target = resolve_navigation_target("docs", &dir).unwrap();

        assert_eq!(target, PathNavigationTarget::Directory(docs));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_navigation_target_resolves_relative_file_against_current_dir() {
        let dir = test_dir("resolve-file");
        let docs = dir.join("docs");
        let file = docs.join("report.txt");
        fs::create_dir_all(&docs).unwrap();
        fs::write(&file, "report").unwrap();

        let target = resolve_navigation_target("docs/report.txt", &dir).unwrap();

        assert_eq!(
            target,
            PathNavigationTarget::File {
                path: file,
                parent_dir: docs,
            }
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_navigation_target_reports_missing_paths() {
        let dir = test_dir("resolve-missing");
        fs::create_dir_all(&dir).unwrap();

        let error = resolve_navigation_target("missing-path", &dir).unwrap_err();

        assert_eq!(error, "Path not found: missing-path");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn short_path_label_truncates_long_paths_to_suffix() {
        let label = short_path_label(Path::new(
            "/very/long/path/that/keeps/going/for/a/while/project",
        ));

        assert_eq!(label, "…eps/going/for/a/while/project");
        assert_eq!(label.chars().count(), 30);
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("files-rusted-{name}-{unique}"))
    }
}
