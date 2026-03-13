use anyhow::{anyhow, bail, Result};

const BEGIN_PATCH: &str = "*** Begin Patch";
const END_PATCH: &str = "*** End Patch";
const ADD_FILE: &str = "*** Add File: ";
const DELETE_FILE: &str = "*** Delete File: ";
const UPDATE_FILE: &str = "*** Update File: ";
const MOVE_TO: &str = "*** Move to: ";
const END_OF_FILE: &str = "*** End of File";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PatchOperation {
    Add {
        path: String,
        content: String,
    },
    Delete {
        path: String,
    },
    Update {
        path: String,
        move_to: Option<String>,
        hunks: Vec<PatchHunk>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PatchHunk {
    pub lines: Vec<PatchLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PatchLine {
    Context(String),
    Delete(String),
    Add(String),
}

pub(crate) fn parse_patch_text(input: &str) -> Result<Vec<PatchOperation>> {
    let lines = input.lines().collect::<Vec<_>>();
    if lines.first().copied() != Some(BEGIN_PATCH) {
        bail!("patch must start with '{BEGIN_PATCH}'");
    }

    let mut index = 1usize;
    let mut operations = Vec::new();
    while index < lines.len() {
        let line = lines[index];
        if line == END_PATCH {
            if operations.is_empty() {
                bail!("patch contained no operations");
            }
            return Ok(operations);
        }
        if let Some(path) = line.strip_prefix(ADD_FILE) {
            index += 1;
            let mut content = Vec::new();
            while index < lines.len() && !is_patch_header(lines[index]) {
                let add_line = lines[index];
                let value = add_line
                    .strip_prefix('+')
                    .ok_or_else(|| anyhow!("add file lines must start with '+'"))?;
                content.push(value.to_string());
                index += 1;
            }
            operations.push(PatchOperation::Add {
                path: path.to_string(),
                content: render_patch_content(&content),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix(DELETE_FILE) {
            operations.push(PatchOperation::Delete {
                path: path.to_string(),
            });
            index += 1;
            continue;
        }
        if let Some(path) = line.strip_prefix(UPDATE_FILE) {
            index += 1;
            let mut move_to = None;
            if index < lines.len() {
                if let Some(destination) = lines[index].strip_prefix(MOVE_TO) {
                    move_to = Some(destination.to_string());
                    index += 1;
                }
            }

            let mut hunks = Vec::new();
            while index < lines.len() && !is_patch_header(lines[index]) {
                let header = lines[index];
                if !header.starts_with("@@") {
                    bail!("invalid update hunk header '{header}'");
                }
                index += 1;
                let mut hunk_lines = Vec::new();
                while index < lines.len() {
                    let line = lines[index];
                    if line.starts_with("@@") || is_patch_header(line) {
                        break;
                    }
                    if line == END_OF_FILE {
                        index += 1;
                        continue;
                    }
                    let patch_line = match line.chars().next() {
                        Some(' ') => PatchLine::Context(line[1..].to_string()),
                        Some('-') => PatchLine::Delete(line[1..].to_string()),
                        Some('+') => PatchLine::Add(line[1..].to_string()),
                        _ => bail!("invalid patch line '{line}'"),
                    };
                    hunk_lines.push(patch_line);
                    index += 1;
                }
                if hunk_lines.is_empty() {
                    bail!("update hunk for '{path}' was empty");
                }
                hunks.push(PatchHunk { lines: hunk_lines });
            }
            if hunks.is_empty() && move_to.is_none() {
                bail!("update for '{path}' contained no hunks");
            }
            operations.push(PatchOperation::Update {
                path: path.to_string(),
                move_to,
                hunks,
            });
            continue;
        }

        bail!("invalid patch header '{line}'");
    }

    bail!("patch was missing '{END_PATCH}'")
}

pub(crate) fn apply_hunks_to_text(original: &str, hunks: &[PatchHunk]) -> Result<String> {
    let lines = original.lines().map(ToOwned::to_owned).collect::<Vec<_>>();
    let mut cursor = 0usize;
    let mut output = Vec::new();

    for hunk in hunks {
        let old_chunk = hunk
            .lines
            .iter()
            .filter_map(|line| match line {
                PatchLine::Context(text) | PatchLine::Delete(text) => Some(text.clone()),
                PatchLine::Add(_) => None,
            })
            .collect::<Vec<_>>();

        let start = find_hunk_start(&lines, cursor, &old_chunk)
            .ok_or_else(|| anyhow!("failed to find hunk context in target file"))?;
        output.extend(lines[cursor..start].iter().cloned());

        let mut local = start;
        for patch_line in &hunk.lines {
            match patch_line {
                PatchLine::Context(text) => {
                    ensure_line(&lines, local, text)?;
                    output.push(text.clone());
                    local += 1;
                }
                PatchLine::Delete(text) => {
                    ensure_line(&lines, local, text)?;
                    local += 1;
                }
                PatchLine::Add(text) => output.push(text.clone()),
            }
        }
        cursor = local;
    }

    output.extend(lines[cursor..].iter().cloned());
    Ok(render_patch_content(&output))
}

fn render_patch_content(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn find_hunk_start(lines: &[String], cursor: usize, chunk: &[String]) -> Option<usize> {
    if chunk.is_empty() {
        return Some(cursor.min(lines.len()));
    }

    (cursor..=lines.len().saturating_sub(chunk.len()))
        .find(|start| slice_matches(&lines[*start..(*start + chunk.len())], chunk))
}

fn slice_matches(candidate: &[String], chunk: &[String]) -> bool {
    candidate.len() == chunk.len()
        && candidate
            .iter()
            .zip(chunk.iter())
            .all(|(candidate, expected)| candidate == expected)
}

fn ensure_line(lines: &[String], index: usize, expected: &str) -> Result<()> {
    let actual = lines
        .get(index)
        .ok_or_else(|| anyhow!("patch referenced a line past the end of the file"))?;
    if actual != expected {
        bail!("patch context mismatch: expected '{expected}', found '{actual}'");
    }
    Ok(())
}

fn is_patch_header(line: &str) -> bool {
    line == END_PATCH
        || line.starts_with(ADD_FILE)
        || line.starts_with(DELETE_FILE)
        || line.starts_with(UPDATE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_add_update_delete_patch() {
        let operations = parse_patch_text(
            "*** Begin Patch\n*** Add File: hello.txt\n+hello\n*** Delete File: old.txt\n*** Update File: notes.txt\n@@\n-old\n+new\n*** End Patch",
        )
        .unwrap();

        assert_eq!(operations.len(), 3);
        assert!(matches!(operations[0], PatchOperation::Add { .. }));
        assert!(matches!(operations[1], PatchOperation::Delete { .. }));
        assert!(matches!(operations[2], PatchOperation::Update { .. }));
    }

    #[test]
    fn applies_update_hunks() {
        let operations = parse_patch_text(
            "*** Begin Patch\n*** Update File: notes.txt\n@@\n alpha\n-beta\n+gamma\n*** End Patch",
        )
        .unwrap();

        let PatchOperation::Update { hunks, .. } = &operations[0] else {
            panic!("expected update operation");
        };
        let updated = apply_hunks_to_text("alpha\nbeta\n", hunks).unwrap();
        assert_eq!(updated, "alpha\ngamma\n");
    }

    #[test]
    fn rejects_missing_context() {
        let operations = parse_patch_text(
            "*** Begin Patch\n*** Update File: notes.txt\n@@\n-missing\n+changed\n*** End Patch",
        )
        .unwrap();

        let PatchOperation::Update { hunks, .. } = &operations[0] else {
            panic!("expected update operation");
        };
        let error = apply_hunks_to_text("alpha\nbeta\n", hunks).unwrap_err();
        assert!(error.to_string().contains("failed to find hunk context"));
    }
}
