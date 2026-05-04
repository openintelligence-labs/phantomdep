//! Parse pip-style requirements files (requirements.txt, constraints.txt).

use std::collections::BTreeSet;

/// Extract distribution names from the contents of a requirements.txt-style file.
/// Honors only the things attackers can plant: bare `name`, `name==1.0`, etc.
/// Skips `-r recursive.txt`, URLs, paths, and comments. Recursion is left to
/// the caller (we don't read additional files here).
pub fn extract_requirements(source: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();

    for raw in source.lines() {
        let line = raw.split('#').next().unwrap_or(raw).trim();
        if line.is_empty() {
            continue;
        }
        // Strip pip-specific options.
        if line.starts_with('-') {
            continue;
        }
        // Skip URLs, paths, VCS specifiers.
        if line.contains("://")
            || line.starts_with('.')
            || line.starts_with('/')
            || line.starts_with("git+")
            || line.starts_with("file:")
        {
            continue;
        }
        // Cut at PEP 508 separators.
        let cut = line
            .find(['=', '>', '<', '!', '~', ';', ' ', '@'])
            .unwrap_or(line.len());
        let head = &line[..cut];
        let name = head.split('[').next().unwrap_or(head).trim();
        if !name.is_empty() {
            names.insert(name.to_lowercase());
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &str) -> Vec<String> {
        extract_requirements(s).into_iter().collect()
    }

    #[test]
    fn extracts_simple_names() {
        let names = extract("requests\nfastapi==0.110\nuvicorn[standard]>=0.30\n");
        assert_eq!(names, vec!["fastapi", "requests", "uvicorn"]);
    }

    #[test]
    fn skips_comments_and_options() {
        let names = extract(
            "# a comment\n--index-url https://pypi.org/simple\n-r other.txt\nrequests\n",
        );
        assert_eq!(names, vec!["requests"]);
    }

    #[test]
    fn skips_urls_and_paths() {
        let names = extract(
            "git+https://github.com/foo/bar\n./local-pkg\nhttps://example.com/x.whl\nrequests\n",
        );
        assert_eq!(names, vec!["requests"]);
    }

    #[test]
    fn lowercases_names() {
        let names = extract("Django\nFlask\n");
        assert_eq!(names, vec!["django", "flask"]);
    }

    #[test]
    fn handles_env_markers() {
        let names = extract("requests; python_version >= '3.8'\nflask\n");
        assert_eq!(names, vec!["flask", "requests"]);
    }
}
