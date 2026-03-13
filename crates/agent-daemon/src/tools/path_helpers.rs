use super::*;

pub(super) fn resolve_existing_path(context: &ToolContext, path: Option<&str>) -> Result<PathBuf> {
    let resolved = normalize_path(join_to_cwd(&context.cwd, path.unwrap_or(".")))?;
    ensure_readable_path(context, &resolved)?;
    Ok(resolved)
}

pub(super) fn resolve_writable_path(context: &ToolContext, path: &str) -> Result<PathBuf> {
    let resolved = normalize_path(join_to_cwd(&context.cwd, path))?;
    ensure_writable_path(context, &resolved)?;
    Ok(resolved)
}

pub(super) fn ensure_readable_path(context: &ToolContext, path: &Path) -> Result<()> {
    if !path_is_trusted(&context.trust_policy, &context.autonomy, &context.cwd, path) {
        bail!("path '{}' is outside trusted roots", path.display());
    }
    Ok(())
}

pub(super) fn ensure_writable_path(context: &ToolContext, path: &Path) -> Result<()> {
    ensure_readable_path(context, path)?;
    if is_self_edit_path(path)?
        && (!context.background_self_edit_allowed
            || !allow_self_edit(&context.trust_policy, &context.autonomy))
    {
        bail!("writing inside the agent install tree requires self-edit permission");
    }
    Ok(())
}

pub(super) fn join_to_cwd(cwd: &Path, path: &str) -> PathBuf {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        candidate
    } else {
        cwd.join(candidate)
    }
}

pub(super) fn normalize_path(path: PathBuf) -> Result<PathBuf> {
    if path.exists() {
        return path
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", path.display()));
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    if parent.exists() {
        let normalized_parent = parent
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", parent.display()))?;
        if let Some(name) = path.file_name() {
            return Ok(normalized_parent.join(name));
        }
        return Ok(normalized_parent);
    }

    Ok(path)
}

pub(super) fn is_self_edit_path(path: &Path) -> Result<bool> {
    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let exe_dir = exe.parent().unwrap_or_else(|| Path::new(""));
    Ok(path.starts_with(exe_dir))
}

pub(super) fn find_paths(
    context: &ToolContext,
    root: &Path,
    current: &Path,
    pattern: &str,
    max_results: usize,
    results: &mut Vec<String>,
    visited: &mut HashSet<PathBuf>,
) -> Result<()> {
    if results.len() >= max_results {
        return Ok(());
    }

    let current = normalize_existing_entry(context, current)?;
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
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if wildcard_matches(pattern, &relative)
            || wildcard_matches(pattern, entry.file_name().to_string_lossy().as_ref())
        {
            results.push(relative.clone());
        }

        let file_type = metadata.file_type();
        let target_metadata = if file_type.is_symlink() {
            fs::metadata(&path).ok()
        } else {
            Some(metadata)
        };
        if target_metadata.is_some_and(|target| target.is_dir()) {
            find_paths(
                context,
                root,
                &canonical_path,
                pattern,
                max_results,
                results,
                visited,
            )?;
        }
    }

    Ok(())
}

pub(super) fn normalize_existing_entry(context: &ToolContext, path: &Path) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", path.display()))?;
    ensure_readable_path(context, &canonical)?;
    Ok(canonical)
}

pub(super) fn normalize_descendant_path(
    context: &ToolContext,
    path: &Path,
) -> Result<Option<PathBuf>> {
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(_) => return Ok(None),
    };
    if path_is_trusted(
        &context.trust_policy,
        &context.autonomy,
        &context.cwd,
        &canonical,
    ) {
        Ok(Some(canonical))
    } else {
        Ok(None)
    }
}

pub(super) fn wildcard_matches(pattern: &str, candidate: &str) -> bool {
    if pattern.contains('*') || pattern.contains('?') {
        return wildcard_match_inner(
            &pattern.chars().collect::<Vec<_>>(),
            &candidate.chars().collect::<Vec<_>>(),
            0,
            0,
        );
    }

    candidate.contains(pattern)
}

fn wildcard_match_inner(pattern: &[char], text: &[char], p: usize, t: usize) -> bool {
    if p == pattern.len() {
        return t == text.len();
    }

    match pattern[p] {
        '*' => {
            for next in t..=text.len() {
                if wildcard_match_inner(pattern, text, p + 1, next) {
                    return true;
                }
            }
            false
        }
        '?' => t < text.len() && wildcard_match_inner(pattern, text, p + 1, t + 1),
        ch => t < text.len() && ch == text[t] && wildcard_match_inner(pattern, text, p + 1, t + 1),
    }
}

pub(super) fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create directory {}", destination.display()))?;
    for entry in fs::read_dir(source)
        .with_context(|| format!("failed to read directory {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.metadata()?.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create parent directory {}", parent.display())
                })?;
            }
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

pub(super) fn remove_existing_path(path: &Path) -> Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    }
}
