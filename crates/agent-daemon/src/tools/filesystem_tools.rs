use std::collections::HashSet;

use super::argument_helpers::{optional_bool, optional_string, optional_u64, required_string};
use super::path_helpers::{
    copy_dir_recursive, ensure_writable_path, find_paths, normalize_descendant_path,
    normalize_existing_entry, remove_existing_path, resolve_existing_path, resolve_writable_path,
};
use super::*;

pub(super) fn list_dir(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_existing_path(context, optional_string(args, "path"))?;
    let max_entries = optional_u64(args, "max_entries")
        .unwrap_or(MAX_DIRECTORY_ENTRIES as u64)
        .min(MAX_DIRECTORY_ENTRIES as u64) as usize;
    let mut entries = fs::read_dir(&path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    let mut output = Vec::new();
    for entry in entries.into_iter().take(max_entries) {
        let metadata = entry.metadata().ok();
        let kind = if metadata.as_ref().map(|meta| meta.is_dir()).unwrap_or(false) {
            "dir"
        } else {
            "file"
        };
        let size = metadata.map(|meta| meta.len()).unwrap_or_default();
        output.push(format!(
            "{}\t{}\t{}",
            kind,
            size,
            entry.file_name().to_string_lossy()
        ));
    }

    if output.is_empty() {
        Ok("(empty directory)".to_string())
    } else {
        Ok(output.join("\n"))
    }
}

pub(super) fn read_file(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_existing_path(context, Some(required_string(args, "path")?))?;
    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read text file {}", path.display()))?;
    let start_line = optional_u64(args, "start_line").unwrap_or(1).max(1) as usize;
    let end_line = optional_u64(args, "end_line").map(|value| value.max(1) as usize);

    let mut lines = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if line_number < start_line {
            continue;
        }
        if end_line.is_some_and(|limit| line_number > limit) {
            break;
        }
        lines.push(format!("{line_number}: {line}"));
    }

    if lines.is_empty() {
        Ok("(no lines in requested range)".to_string())
    } else {
        Ok(lines.join("\n"))
    }
}

#[derive(Clone)]
enum PatchBackupKind {
    Missing,
    File(Vec<u8>),
}

#[derive(Default)]
struct PatchRollbackGuard {
    captured: HashSet<PathBuf>,
    backups: Vec<(PathBuf, PatchBackupKind)>,
}

impl PatchRollbackGuard {
    fn capture(&mut self, path: &Path) -> Result<()> {
        if !self.captured.insert(path.to_path_buf()) {
            return Ok(());
        }
        let backup = if path.exists() {
            let metadata =
                fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
            if metadata.is_dir() {
                bail!("cannot apply patch to directory '{}'", path.display());
            }
            PatchBackupKind::File(
                fs::read(path).with_context(|| format!("failed to read {}", path.display()))?,
            )
        } else {
            PatchBackupKind::Missing
        };
        self.backups.push((path.to_path_buf(), backup));
        Ok(())
    }

    fn restore(self) -> Result<()> {
        for (path, backup) in self.backups.into_iter().rev() {
            match backup {
                PatchBackupKind::Missing => {
                    if path.exists() {
                        fs::remove_file(&path)
                            .with_context(|| format!("failed to remove {}", path.display()))?;
                    }
                }
                PatchBackupKind::File(content) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("failed to create parent directory {}", parent.display())
                        })?;
                    }
                    fs::write(&path, content)
                        .with_context(|| format!("failed to restore {}", path.display()))?;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn apply_patch_tool(context: &ToolContext, args: &Value) -> Result<String> {
    let patch = required_string(args, "patch")?;
    let operations = parse_patch_text(patch)?;
    let mut summaries = Vec::new();
    let mut rollback = PatchRollbackGuard::default();

    let result: Result<()> = (|| {
        for operation in operations {
            match operation {
                PatchOperation::Add { path, content } => {
                    let resolved = resolve_writable_path(context, &path)?;
                    if resolved.exists() {
                        bail!("cannot add '{}': file already exists", resolved.display());
                    }
                    rollback.capture(&resolved)?;
                    if let Some(parent) = resolved.parent() {
                        fs::create_dir_all(parent).with_context(|| {
                            format!("failed to create parent directory {}", parent.display())
                        })?;
                    }
                    fs::write(&resolved, content.as_bytes())
                        .with_context(|| format!("failed to write {}", resolved.display()))?;
                    summaries.push(format!("added {}", resolved.display()));
                }
                PatchOperation::Delete { path } => {
                    let resolved = resolve_existing_path(context, Some(&path))?;
                    ensure_writable_path(context, &resolved)?;
                    rollback.capture(&resolved)?;
                    let metadata = fs::metadata(&resolved)
                        .with_context(|| format!("failed to stat {}", resolved.display()))?;
                    if metadata.is_dir() {
                        bail!(
                            "cannot delete directory '{}' with apply_patch",
                            resolved.display()
                        );
                    }
                    fs::remove_file(&resolved)
                        .with_context(|| format!("failed to remove {}", resolved.display()))?;
                    summaries.push(format!("deleted {}", resolved.display()));
                }
                PatchOperation::Update {
                    path,
                    move_to,
                    hunks,
                } => {
                    let source = resolve_existing_path(context, Some(&path))?;
                    ensure_writable_path(context, &source)?;
                    rollback.capture(&source)?;
                    let original = fs::read_to_string(&source)
                        .with_context(|| format!("failed to read {}", source.display()))?;
                    let updated = if hunks.is_empty() {
                        original
                    } else {
                        apply_hunks_to_text(&original, &hunks)?
                    };
                    let destination = if let Some(move_to) = move_to {
                        let destination = resolve_writable_path(context, &move_to)?;
                        if destination != source {
                            rollback.capture(&destination)?;
                            if destination.exists() {
                                bail!(
                                    "cannot move '{}' to '{}': destination already exists",
                                    source.display(),
                                    destination.display()
                                );
                            }
                            if let Some(parent) = destination.parent() {
                                fs::create_dir_all(parent).with_context(|| {
                                    format!(
                                        "failed to create parent directory {}",
                                        parent.display()
                                    )
                                })?;
                            }
                        }
                        destination
                    } else {
                        source.clone()
                    };
                    fs::write(&source, updated.as_bytes())
                        .with_context(|| format!("failed to write {}", source.display()))?;
                    if destination != source {
                        fs::rename(&source, &destination).with_context(|| {
                            format!(
                                "failed to move {} to {}",
                                source.display(),
                                destination.display()
                            )
                        })?;
                        summaries.push(format!(
                            "updated {} and moved to {}",
                            source.display(),
                            destination.display()
                        ));
                    } else {
                        summaries.push(format!("updated {}", source.display()));
                    }
                }
            }
        }
        Ok(())
    })();

    if let Err(error) = result {
        if let Err(restore_error) = rollback.restore() {
            return Err(anyhow!(
                "failed to apply patch: {error:#}; rollback also failed: {restore_error:#}"
            ));
        }
        return Err(error);
    }

    Ok(summaries.join("\n"))
}

pub(super) fn write_file(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_writable_path(context, required_string(args, "path")?)?;
    let content = required_string(args, "content")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }
    fs::write(&path, content.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(format!(
        "wrote {} bytes to {}",
        content.len(),
        path.display()
    ))
}

pub(super) fn append_file(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_writable_path(context, required_string(args, "path")?)?;
    let content = required_string(args, "content")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("failed to append to {}", path.display()))?;
    Ok(format!(
        "appended {} bytes to {}",
        content.len(),
        path.display()
    ))
}

pub(super) fn replace_in_file(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_existing_path(context, Some(required_string(args, "path")?))?;
    ensure_writable_path(context, &path)?;
    let old = required_string(args, "old")?;
    let new = required_string(args, "new")?;
    let replace_all = optional_bool(args, "replace_all").unwrap_or(false);
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;

    if !content.contains(old) {
        bail!("text to replace was not found in {}", path.display());
    }

    let updated = if replace_all {
        content.replace(old, new)
    } else {
        content.replacen(old, new, 1)
    };
    let replaced = if replace_all {
        content.matches(old).count()
    } else {
        1
    };
    fs::write(&path, updated.as_bytes())
        .with_context(|| format!("failed to update {}", path.display()))?;
    Ok(format!(
        "replaced {} occurrence(s) in {}",
        replaced,
        path.display()
    ))
}

pub(super) fn search_files(context: &ToolContext, args: &Value) -> Result<String> {
    let root = resolve_existing_path(context, optional_string(args, "path"))?;
    let query = required_string(args, "query")?;
    let max_results = optional_u64(args, "max_results")
        .unwrap_or(MAX_SEARCH_RESULTS as u64)
        .min(MAX_SEARCH_RESULTS as u64) as usize;
    let mut results = Vec::new();
    let mut visited = HashSet::new();
    search_dir(
        context,
        &root,
        query,
        max_results,
        &mut results,
        &mut visited,
    )?;

    if results.is_empty() {
        Ok("no matches found".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

pub(super) fn find_files(context: &ToolContext, args: &Value) -> Result<String> {
    let root = resolve_existing_path(context, optional_string(args, "path"))?;
    let pattern = required_string(args, "pattern")?;
    let max_results = optional_u64(args, "max_results")
        .unwrap_or(MAX_FIND_RESULTS as u64)
        .min(MAX_FIND_RESULTS as u64) as usize;
    let mut results = Vec::new();
    let mut visited = HashSet::new();
    find_paths(
        context,
        &root,
        &root,
        pattern,
        max_results,
        &mut results,
        &mut visited,
    )?;

    if results.is_empty() {
        Ok("no matches found".to_string())
    } else {
        Ok(results.join("\n"))
    }
}

fn search_dir(
    context: &ToolContext,
    root: &Path,
    query: &str,
    max_results: usize,
    results: &mut Vec<String>,
    visited: &mut HashSet<PathBuf>,
) -> Result<()> {
    if results.len() >= max_results {
        return Ok(());
    }

    let current = normalize_existing_entry(context, root)?;
    if !visited.insert(current.clone()) {
        return Ok(());
    }

    for entry in fs::read_dir(&current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
    {
        if results.len() >= max_results {
            break;
        }

        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        let Some(canonical_path) = normalize_descendant_path(context, &path)? else {
            continue;
        };
        let file_type = metadata.file_type();
        let target_metadata = if file_type.is_symlink() {
            fs::metadata(&path).ok()
        } else {
            Some(metadata)
        };
        let Some(target_metadata) = target_metadata else {
            continue;
        };
        if target_metadata.is_dir() {
            search_dir(
                context,
                &canonical_path,
                query,
                max_results,
                results,
                visited,
            )?;
            continue;
        }
        if !target_metadata.is_file() || target_metadata.len() as usize > MAX_SEARCH_FILE_BYTES {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        for (index, line) in content.lines().enumerate() {
            if line.contains(query) {
                results.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                if results.len() >= max_results {
                    break;
                }
            }
        }
    }

    Ok(())
}

pub(super) fn make_dir(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_writable_path(context, required_string(args, "path")?)?;
    fs::create_dir_all(&path)
        .with_context(|| format!("failed to create directory {}", path.display()))?;
    Ok(format!("created {}", path.display()))
}

pub(super) fn copy_path(context: &ToolContext, args: &Value) -> Result<String> {
    let source = resolve_existing_path(context, Some(required_string(args, "source")?))?;
    let destination = resolve_writable_path(context, required_string(args, "destination")?)?;
    let overwrite = optional_bool(args, "overwrite").unwrap_or(false);
    let source_metadata =
        fs::metadata(&source).with_context(|| format!("failed to stat {}", source.display()))?;

    if destination == source {
        bail!("source and destination are the same path");
    }
    if source_metadata.is_dir() && destination.starts_with(&source) {
        bail!("cannot copy a directory into itself");
    }

    if destination.exists() {
        if !overwrite {
            bail!("destination '{}' already exists", destination.display());
        }
        remove_existing_path(&destination)?;
    }

    if source_metadata.is_dir() {
        copy_dir_recursive(&source, &destination)?;
    } else {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directory {}", parent.display())
            })?;
        }
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "failed to copy {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }

    Ok(format!(
        "copied {} to {}",
        source.display(),
        destination.display()
    ))
}

pub(super) fn move_path(context: &ToolContext, args: &Value) -> Result<String> {
    let source = resolve_existing_path(context, Some(required_string(args, "source")?))?;
    let destination = resolve_writable_path(context, required_string(args, "destination")?)?;
    let overwrite = optional_bool(args, "overwrite").unwrap_or(false);
    let source_metadata =
        fs::metadata(&source).with_context(|| format!("failed to stat {}", source.display()))?;

    if destination == source {
        bail!("source and destination are the same path");
    }
    if source_metadata.is_dir() && destination.starts_with(&source) {
        bail!("cannot move a directory into itself");
    }

    if destination.exists() {
        if !overwrite {
            bail!("destination '{}' already exists", destination.display());
        }
        remove_existing_path(&destination)?;
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }

    match fs::rename(&source, &destination) {
        Ok(()) => {}
        Err(_) => {
            if source_metadata.is_dir() {
                copy_dir_recursive(&source, &destination)?;
                fs::remove_dir_all(&source)
                    .with_context(|| format!("failed to remove {}", source.display()))?;
            } else {
                fs::copy(&source, &destination).with_context(|| {
                    format!(
                        "failed to move {} to {}",
                        source.display(),
                        destination.display()
                    )
                })?;
                fs::remove_file(&source)
                    .with_context(|| format!("failed to remove {}", source.display()))?;
            }
        }
    }

    Ok(format!(
        "moved {} to {}",
        source.display(),
        destination.display()
    ))
}

pub(super) fn delete_path(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_existing_path(context, Some(required_string(args, "path")?))?;
    ensure_writable_path(context, &path)?;
    let recursive = optional_bool(args, "recursive").unwrap_or(false);
    let metadata =
        fs::metadata(&path).with_context(|| format!("failed to stat {}", path.display()))?;

    if metadata.is_dir() {
        if !recursive {
            bail!(
                "'{}' is a directory; pass recursive=true to delete it",
                path.display()
            );
        }
        fs::remove_dir_all(&path)
            .with_context(|| format!("failed to remove {}", path.display()))?;
    } else {
        fs::remove_file(&path).with_context(|| format!("failed to remove {}", path.display()))?;
    }

    Ok(format!("deleted {}", path.display()))
}

pub(super) fn stat_path(context: &ToolContext, args: &Value) -> Result<String> {
    let path = resolve_existing_path(context, Some(required_string(args, "path")?))?;
    let metadata = fs::metadata(&path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let modified = metadata
        .modified()
        .ok()
        .map(DateTime::<Utc>::from)
        .map(|date| date.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());
    Ok(format!(
        "path={}\nkind={}\nsize={}\nreadonly={}\nmodified={}",
        path.display(),
        if metadata.is_dir() { "dir" } else { "file" },
        metadata.len(),
        metadata.permissions().readonly(),
        modified
    ))
}
