//! Field-accurate parsing and editing of `/etc/fstab` swap entries.
//!
//! fstab (and `/proc/swaps`) encode space, tab, newline and backslash in path fields as
//! octal escapes (`\040`, `\011`, `\012`, `\134`). Matching must compare the decoded first
//! field exactly; substring checks confuse `/swapfile` with `/var/swapfile` or comments.

/// A non-comment fstab line whose filesystem type is `swap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FstabSwapEntry {
    /// Decoded spec (first field).
    pub spec: String,
    /// Raw mount options (fourth field), `defaults` when absent.
    pub options: String,
}

/// Decode fstab/`/proc/swaps` octal escapes (`\NNN`). Invalid sequences stay literal.
pub fn decode_fstab_escapes(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() {
            let digits = &bytes[i + 1..i + 4];
            if digits.iter().all(|d| (b'0'..=b'7').contains(d)) {
                let value = digits
                    .iter()
                    .fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
                if let Ok(byte) = u8::try_from(value) {
                    out.push(byte);
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Encode a value for an fstab field (inverse of [`decode_fstab_escapes`]).
pub fn encode_fstab_field(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            ' ' => out.push_str("\\040"),
            '\t' => out.push_str("\\011"),
            '\n' => out.push_str("\\012"),
            '\\' => out.push_str("\\134"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse one line as a swap entry; `None` for blanks, comments, and non-swap lines.
pub fn parse_swap_line(line: &str) -> Option<FstabSwapEntry> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    if fields.len() < 3 || fields[2] != "swap" {
        return None;
    }
    Some(FstabSwapEntry {
        spec: decode_fstab_escapes(fields[0]),
        options: fields.get(3).copied().unwrap_or("defaults").to_string(),
    })
}

/// All swap entries in an fstab document, in order.
pub fn parse_swap_entries(content: &str) -> Vec<FstabSwapEntry> {
    content.lines().filter_map(parse_swap_line).collect()
}

/// True when `line` is a swap entry whose decoded spec equals `path` exactly.
pub fn line_is_swap_entry_for(line: &str, path: &str) -> bool {
    parse_swap_line(line).is_some_and(|e| e.spec == path)
}

/// The fstab line xzram writes for a swapfile.
pub fn swap_entry_line(path: &str, priority: i32) -> String {
    format!(
        "{} none swap sw,pri={priority} 0 0",
        encode_fstab_field(path)
    )
}

/// Insert or replace the swap entry for `path`, updating its priority.
///
/// The first existing entry is replaced in place; later duplicates are dropped. Comments and
/// unrelated lines are preserved verbatim.
pub fn upsert_swap_entry(content: &str, path: &str, priority: i32) -> String {
    let new_line = swap_entry_line(path, priority);
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for line in content.lines() {
        if line_is_swap_entry_for(line, path) {
            if !replaced {
                out.push(new_line.clone());
                replaced = true;
            }
            continue;
        }
        out.push(line.to_string());
    }
    if !replaced {
        out.push(new_line);
    }
    join_lines(out)
}

/// Remove every swap entry for `path`; other lines are preserved verbatim.
pub fn remove_swap_entry(content: &str, path: &str) -> String {
    let out: Vec<String> = content
        .lines()
        .filter(|line| !line_is_swap_entry_for(line, path))
        .map(str::to_string)
        .collect();
    join_lines(out)
}

fn join_lines(lines: Vec<String>) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

/// True when a `/proc/swaps` document lists `path` as an active file-backed swap.
pub fn proc_swaps_lists_file(content: &str, path: &str) -> bool {
    content.lines().skip(1).any(|line| {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            return false;
        };
        let typ = parts.next().unwrap_or("");
        decode_fstab_escapes(name) == path && typ.eq_ignore_ascii_case("file")
    })
}

/// True when a `/proc/swaps` document lists `path` as active swap of any type.
pub fn proc_swaps_lists_path(content: &str, path: &str) -> bool {
    content.lines().skip(1).any(|line| {
        line.split_whitespace()
            .next()
            .is_some_and(|name| decode_fstab_escapes(name) == path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_and_encodes_escapes() {
        assert_eq!(decode_fstab_escapes("/swap/my\\040file"), "/swap/my file");
        assert_eq!(decode_fstab_escapes("/a\\134b\\011c"), "/a\\b\tc");
        assert_eq!(decode_fstab_escapes("/a\\9xx"), "/a\\9xx");
        assert_eq!(decode_fstab_escapes("/trail\\04"), "/trail\\04");
        let raw = "/swap/a b\\c";
        assert_eq!(decode_fstab_escapes(&encode_fstab_field(raw)), raw);
    }

    #[test]
    fn exact_spec_match_ignores_comments_and_prefixes() {
        let fstab = "# /swapfile none swap sw 0 0\n/var/swapfile none swap sw,pri=5 0 0\n";
        assert!(!parse_swap_entries(fstab)
            .iter()
            .any(|e| e.spec == "/swapfile"));
        assert!(line_is_swap_entry_for(
            "/swap/my\\040file none swap sw 0 0",
            "/swap/my file"
        ));
        assert!(!line_is_swap_entry_for(
            "/swapfile /mnt ext4 loop 0 0",
            "/swapfile"
        ));
    }

    #[test]
    fn upsert_appends_when_only_similar_paths_exist() {
        let fstab = "# /swapfile old\n/var/swapfile none swap sw,pri=5 0 0\n";
        let out = upsert_swap_entry(fstab, "/swapfile", 20);
        assert_eq!(
            out,
            "# /swapfile old\n/var/swapfile none swap sw,pri=5 0 0\n/swapfile none swap sw,pri=20 0 0\n"
        );
    }

    #[test]
    fn upsert_replaces_priority_and_dedups() {
        let fstab = "UUID=x / ext4 defaults 0 1\n/swapfile none swap sw,pri=10 0 0\n/swapfile none swap sw 0 0\n";
        let out = upsert_swap_entry(fstab, "/swapfile", 50);
        assert_eq!(
            out,
            "UUID=x / ext4 defaults 0 1\n/swapfile none swap sw,pri=50 0 0\n"
        );
        let entries = parse_swap_entries(&out);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].options, "sw,pri=50");
    }

    #[test]
    fn upsert_into_empty() {
        assert_eq!(
            upsert_swap_entry("", "/swap/swapfile", 10),
            "/swap/swapfile none swap sw,pri=10 0 0\n"
        );
    }

    #[test]
    fn remove_is_exact() {
        let fstab = "/swapfile none swap sw 0 0\n/swapfile2 none swap sw 0 0\n# /swapfile\n";
        assert_eq!(
            remove_swap_entry(fstab, "/swapfile"),
            "/swapfile2 none swap sw 0 0\n# /swapfile\n"
        );
    }

    #[test]
    fn proc_swaps_decoding() {
        let swaps = "Filename\tType\tSize\tUsed\tPriority\n/swap/a\\040b file 1024 0 -2\n/dev/zram0 partition 1 0 100\n";
        assert!(proc_swaps_lists_file(swaps, "/swap/a b"));
        assert!(!proc_swaps_lists_file(swaps, "/dev/zram0"));
        assert!(proc_swaps_lists_path(swaps, "/dev/zram0"));
        assert!(!proc_swaps_lists_path(swaps, "/swap/a"));
    }
}
