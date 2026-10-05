use std::path::Path;

// None retains conservative operand checks for unsupported/source-derived forms.
pub(super) fn write_targets(tokens: &[String]) -> Option<Vec<String>> {
    let executable = Path::new(tokens.first()?)
        .file_name()?
        .to_str()?
        .to_ascii_lowercase();
    let gnu = matches!(executable.as_str(), "gcp" | "gcp.exe");
    let mut sources = Vec::new();
    let mut targets = Vec::new();
    let mut positional = false;
    let mut index = 1;
    while index < tokens.len() {
        let token = &tokens[index];
        if positional || !token.starts_with('-') || token == "-" {
            sources.push(token.clone());
        } else if token == "--" {
            positional = true;
        } else if let Some(long) = token.strip_prefix("--") {
            let (option, attached) = long
                .split_once('=')
                .map_or((long, None), |(name, value)| (name, Some(value)));
            if option.is_empty()
                || "parents".starts_with(option)
                || option == "path"
                || "link".starts_with(option)
            {
                return None;
            }
            let required = ["suffix", "sparse", "no-preserve", "target-directory"];
            let names: Vec<_> = required
                .into_iter()
                .filter(|name| name.starts_with(option))
                .collect();
            if names.len() > 1 {
                return None;
            }
            if let Some(name) = names.first() {
                let value = if let Some(value) = attached {
                    value.to_string()
                } else {
                    index += 1;
                    tokens.get(index)?.clone()
                };
                if *name == "target-directory" {
                    targets.push(value);
                }
            }
            // Optional arguments require '=', so the next token remains an operand.
        } else {
            // BSD -S takes no value; GNU -S consumes a suffix. Avoid excluding
            // a possible write operand or hiding BSD hardlink flags in a cluster.
            if !gnu && token.contains('S') {
                return None;
            }
            for (offset, flag) in token[1..].char_indices() {
                if flag == 'l' {
                    return None;
                }
                if !matches!(flag, 'S' | 't') {
                    continue;
                }
                let tail = &token[offset + 2..];
                let value = if tail.is_empty() {
                    index += 1;
                    tokens.get(index)?.clone()
                } else {
                    tail.to_string()
                };
                if flag == 't' {
                    targets.push(value);
                }
                break;
            }
        }
        index += 1;
    }
    if targets.is_empty() {
        if sources.len() < 2 {
            return None;
        }
        targets.push(sources.pop()?);
    }
    // Directory copies can derive many descendant writes (including source/.).
    // Retain the prior conservative checks until those mappings are modeled.
    if sources.iter().any(|source| Path::new(source).is_dir()) {
        return None;
    }
    // Existing directory destinations write one child for each source basename.
    // Inspect those children too so an existing destination symlink is not missed.
    let mut writes = targets.clone();
    for target in targets {
        if Path::new(&target).is_dir() {
            for source in &sources {
                let leaf = Path::new(source).file_name()?;
                writes.push(Path::new(&target).join(leaf).to_string_lossy().into_owned());
            }
        }
    }
    Some(writes)
}
