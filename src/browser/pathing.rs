use super::{DirectoryEntry, SidebarEntry};
use crate::BreadcrumbEntry;
use jwalk::WalkDir;
use slint::SharedString;
use std::{
    env,
    path::{Path, PathBuf},
};
#[cfg(test)]
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn build_sidebar_entries(start_dir: &Path) -> (Vec<SidebarEntry>, Vec<PathBuf>) {
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
            let size_bytes = if is_dir { 0 } else { metadata.len() };

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
                size_bytes,
            })
        })
        .collect::<Vec<_>>();

    Ok(entries)
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
    if paths.iter().any(|existing| existing == &path) {
        return;
    }

    entries.push(SidebarEntry {
        label: SharedString::from(label),
        caption: SharedString::from(short_path_label(&path)),
    });
    paths.push(path);
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
