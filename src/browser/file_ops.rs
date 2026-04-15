use super::TransferKind;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[cfg(test)]
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn item_name(path: &Path) -> String {
    path.file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub(super) fn unique_child_path(
    parent: &Path,
    base_name: &str,
    extension: Option<&str>,
) -> PathBuf {
    let extension = extension
        .map(str::to_string)
        .filter(|value| !value.is_empty());
    let mut index = 1;

    loop {
        let candidate_name = if index == 1 {
            base_name.to_string()
        } else {
            format!("{} {}", base_name, index)
        };

        let candidate = match extension.as_ref() {
            Some(extension) => parent.join(format!("{}.{}", candidate_name, extension)),
            None => parent.join(candidate_name),
        };

        if !candidate.exists() {
            return candidate;
        }

        index += 1;
    }
}

pub(super) fn copy_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    if source.is_dir() {
        fs::create_dir_all(destination)?;

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let child_source = entry.path();
            let child_destination = destination.join(entry.file_name());
            copy_path(&child_source, &child_destination)?;
        }

        Ok(())
    } else {
        fs::copy(source, destination).map(|_| ())
    }
}

pub(super) fn move_path(source: &Path, destination: &Path) -> std::io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_path(source, destination)?;
            if source.is_dir() {
                fs::remove_dir_all(source)
            } else {
                fs::remove_file(source)
            }
        }
    }
}

pub(super) fn destination_for_transfer(
    kind: TransferKind,
    source: &Path,
    target_dir: &Path,
) -> PathBuf {
    let source_name = item_name(source);
    let original_target = target_dir.join(&source_name);

    if matches!(kind, TransferKind::Cut) && !original_target.exists() {
        return original_target;
    }

    if matches!(kind, TransferKind::Copy)
        && !original_target.exists()
        && source.parent() != Some(target_dir)
    {
        return original_target;
    }

    let (stem, extension) = split_name_parts(source);
    let copy_base = if matches!(kind, TransferKind::Copy) {
        format!("{} Copy", stem)
    } else {
        stem
    };

    unique_child_path(target_dir, &copy_base, extension.as_deref())
}

pub(super) fn launch_path(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path);
        return spawn_detached(command);
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);
        return spawn_detached(command);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        return spawn_detached(command);
    }

    #[allow(unreachable_code)]
    Err(std::io::Error::other(
        "Open is not implemented for this platform",
    ))
}

pub(super) fn format_item_count(count: usize) -> String {
    if count == 1 {
        "1 item".to_string()
    } else {
        format!("{} items", count)
    }
}

fn split_name_parts(path: &Path) -> (String, Option<String>) {
    if path.is_dir() {
        return (item_name(path), None);
    }

    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty());

    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| item_name(path));

    (stem, extension)
}

fn spawn_detached(mut command: Command) -> std::io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("files-rusted-{name}-{unique}"))
    }

    #[test]
    fn unique_child_path_appends_incrementing_suffixes() {
        let dir = test_dir("unique-child-path");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("New File.txt"), "one").unwrap();
        fs::write(dir.join("New File 2.txt"), "two").unwrap();

        let candidate = unique_child_path(&dir, "New File", Some("txt"));

        assert_eq!(candidate.file_name().unwrap(), "New File 3.txt");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn destination_for_transfer_keeps_name_for_cut_into_new_directory() {
        let source_parent = test_dir("cut-source");
        let target_parent = test_dir("cut-target");
        fs::create_dir_all(&source_parent).unwrap();
        fs::create_dir_all(&target_parent).unwrap();
        let source = source_parent.join("report.txt");
        fs::write(&source, "content").unwrap();

        let destination = destination_for_transfer(TransferKind::Cut, &source, &target_parent);

        assert_eq!(destination, target_parent.join("report.txt"));
        fs::remove_dir_all(&source_parent).unwrap();
        fs::remove_dir_all(&target_parent).unwrap();
    }

    #[test]
    fn destination_for_transfer_creates_copy_suffix_inside_same_directory() {
        let dir = test_dir("copy-suffix");
        fs::create_dir_all(&dir).unwrap();
        let source = dir.join("report.txt");
        fs::write(&source, "content").unwrap();

        let destination = destination_for_transfer(TransferKind::Copy, &source, &dir);

        assert_eq!(destination.file_name().unwrap(), "report Copy.txt");
        fs::remove_dir_all(&dir).unwrap();
    }
}
