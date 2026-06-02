pub fn match_glob(pattern: &str, path: &str) -> bool {
    let pattern_bytes = pattern.as_bytes();
    let path_bytes = path.as_bytes();
    match_glob_bytes(pattern_bytes, path_bytes)
}

fn match_glob_bytes(pattern: &[u8], path: &[u8]) -> bool {
    let mut pi = 0;
    let mut qi = 0;
    let mut star_pi = usize::MAX;
    let mut star_qi = 0;

    while qi < path.len() {
        if pi < pattern.len() {
            match pattern[pi] {
                b'?' => {
                    if path[qi] != b'/' {
                        pi += 1;
                        qi += 1;
                        continue;
                    }
                }
                b'*' => {
                    if pi + 1 < pattern.len() && pattern[pi + 1] == b'*' {
                        star_pi = pi;
                        star_qi = qi;
                        pi += 2;
                        if pi < pattern.len() && pattern[pi] == b'/' {
                            pi += 1;
                        }
                        continue;
                    } else {
                        star_pi = pi;
                        star_qi = qi;
                        pi += 1;
                        continue;
                    }
                }
                b'{' => {
                    if let Some(end) = find_brace_end(pattern, pi) {
                        let alternatives = &pattern[pi + 1..end];
                        let rest_pattern = &pattern[end + 1..];
                        for alt in split_alternatives(alternatives) {
                            let mut new_pattern = Vec::new();
                            new_pattern.extend_from_slice(alt);
                            new_pattern.extend_from_slice(rest_pattern);
                            if match_glob_bytes(&new_pattern, &path[qi..]) {
                                return true;
                            }
                        }
                        return false;
                    }
                }
                c if c == path[qi] => {
                    pi += 1;
                    qi += 1;
                    continue;
                }
                _ => {}
            }
        }

        if star_pi != usize::MAX {
            pi = star_pi + 1;
            star_qi += 1;
            qi = star_qi;
            if pi < pattern.len() && pattern[pi] == b'*' {
                pi += 1;
            }
        } else {
            return false;
        }
    }

    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

fn find_brace_end(pattern: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, item) in pattern.iter().enumerate().skip(start) {
        match *item {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_alternatives(bytes: &[u8]) -> Vec<&[u8]> {
    let mut alternatives = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for i in 0..bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                alternatives.push(&bytes[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    alternatives.push(&bytes[start..]);
    alternatives
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_glob() {
        assert!(match_glob("*.rs", "main.rs"));
        assert!(match_glob("*.rs", "lib.rs"));
        assert!(!match_glob("*.rs", "main.py"));
    }

    #[test]
    fn test_double_star() {
        assert!(match_glob("src/**/*.rs", "src/main.rs"));
        assert!(match_glob("src/**/*.rs", "src/parser/zig.rs"));
        assert!(!match_glob("src/**/*.rs", "test/main.rs"));
    }

    #[test]
    fn test_brace_alternatives() {
        assert!(match_glob("*.{rs,toml}", "main.rs"));
        assert!(match_glob("*.{rs,toml}", "Cargo.toml"));
        assert!(!match_glob("*.{rs,toml}", "main.py"));
    }

    #[test]
    fn test_question_mark() {
        assert!(match_glob("?.rs", "a.rs"));
        assert!(!match_glob("?.rs", "ab.rs"));
    }
}
