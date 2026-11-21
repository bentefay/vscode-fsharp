//! Optional parser-input projection for dense, symbol-free integer arrays.
//! This is not source rewriting: every byte offset and newline is retained.
//! Consumers must use the original source for positions/text and keep the
//! provenance below; array cardinality/value inference from the projection is
//! forbidden. No identifiers, operators, comments or strings are elided.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiteralIsland {
    pub start: usize,
    pub end: usize,
    pub first_start: usize,
    pub first_end: usize,
    pub element_count: usize,
}

pub struct LiteralProjection {
    pub text: String,
    pub islands: Vec<LiteralIsland>,
}

/// Recognize only semicolon-separated decimal Int32 constants, with an optional
/// *adjacent* minus sign. In F#, `-1` is a constant, whereas `- 1` can invoke a
/// user-defined unary operator. Units, suffixes and expressions are excluded.
fn integer_array(
    source: &[u8],
    start: usize,
    min_elements: usize,
    cancelled: &impl Fn() -> bool,
) -> Option<LiteralIsland> {
    let mut i = start + 2;
    let mut count = 0;
    let mut first = (0, 0);
    loop {
        if count % 1024 == 0 && cancelled() {
            return None;
        }
        while source
            .get(i)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            i += 1;
        }
        if source.get(i..i + 2) == Some(b"|]") {
            return (count >= min_elements).then_some(LiteralIsland {
                start,
                end: i + 2,
                first_start: first.0,
                first_end: first.1,
                element_count: count,
            });
        }
        let number_start = i;
        if source.get(i) == Some(&b'-') {
            i += 1;
        }
        let digits_start = i;
        while source.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == digits_start {
            return None;
        }
        std::str::from_utf8(&source[number_start..i])
            .ok()?
            .parse::<i32>()
            .ok()?;
        if count == 0 {
            first = (number_start, i);
        }
        count += 1;
        while source
            .get(i)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            i += 1;
        }
        if source.get(i..i + 2) == Some(b"|]") {
            continue;
        }
        if source.get(i) != Some(&b';') {
            return None;
        }
        i += 1;
    }
}

fn skip_quoted(source: &[u8], quote: usize, verbatim: bool) -> Option<usize> {
    if source.get(quote..quote + 4) == Some(b"\"\"\"\"") {
        return None;
    }
    let triple = !verbatim && source.get(quote..quote + 3) == Some(b"\"\"\"");
    let mut i = quote + if triple { 3 } else { 1 };
    while i < source.len() {
        if triple {
            if source.get(i..i + 3) == Some(b"\"\"\"") {
                return Some(i + 3);
            }
        } else if source[i] == b'"' {
            if verbatim && source.get(i + 1) == Some(&b'"') {
                i += 2;
                continue;
            }
            return Some(i + 1);
        } else if !verbatim && source[i] == b'\\' {
            i += 1;
        }
        i += 1;
    }
    None
}

fn skip_character(source: &[u8], quote: usize) -> Option<usize> {
    let mut i = quote + 1;
    let first = *source.get(i)?;
    if first == b'\\' {
        i += 1;
        i += match *source.get(i)? {
            b'u' => 5,
            b'U' => 9,
            b'x' => 3,
            b'0'..=b'9' => 3,
            _ => 1,
        };
    } else {
        i += if first < 0x80 {
            1
        } else if first < 0xE0 {
            2
        } else if first < 0xF0 {
            3
        } else {
            4
        };
    }
    (source.get(i) == Some(&b'\'')).then_some(i + 1)
}

/// This deliberately declines unsupported lexical states. An ordinary parser
/// remains responsible for such files; refusing projection never proves that a
/// source file has no symbols. Callers may choose a larger minimum island size.
#[cfg(test)]
pub fn project_integer_arrays(source: &str, min_elements: usize) -> Option<LiteralProjection> {
    project_integer_arrays_with_cancel(source, min_elements, || false)
}
pub fn project_integer_arrays_with_cancel(
    source: &str,
    min_elements: usize,
    cancelled: impl Fn() -> bool,
) -> Option<LiteralProjection> {
    let bytes = source.as_bytes();
    let mut islands = Vec::new();
    let mut i = 0;
    let mut attribute_depth = 0u32;
    while i < bytes.len() {
        if cancelled() {
            return None;
        }
        if bytes.get(i..i + 2) == Some(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes.get(i..i + 2) == Some(b"(*") {
            let mut depth = 1u32;
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'"' {
                    return None;
                } // Quoted comment delimiters require the full lexer.
                if bytes.get(i..i + 2) == Some(b"(*") {
                    depth += 1;
                    i += 2;
                } else if bytes.get(i..i + 2) == Some(b"*)") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth != 0 {
                return None;
            }
        } else if bytes.get(i..i + 2) == Some(b"``") {
            i += 2;
            while i < bytes.len() && bytes.get(i..i + 2) != Some(b"``") {
                if matches!(bytes[i], b'\n' | b'\r') {
                    return None;
                }
                i += 1;
            }
            if i >= bytes.len() {
                return None;
            }
            i += 2;
        } else if matches!(bytes[i], b'$' | b'#') {
            return None; // Interpolation/directive context is intentionally not projected.
        } else if bytes.get(i..i + 2) == Some(b"@\"") {
            i = skip_quoted(bytes, i + 1, true)?;
        } else if bytes[i] == b'"' {
            i = skip_quoted(bytes, i, false)?;
        } else if bytes[i] == b'\'' {
            i = skip_character(bytes, i).unwrap_or(i + 1); // Otherwise a type-variable apostrophe.
        } else if bytes.get(i..i + 2) == Some(b"[<") {
            attribute_depth += 1;
            i += 2;
        } else if bytes.get(i..i + 2) == Some(b">]") {
            attribute_depth = attribute_depth.checked_sub(1)?;
            i += 2;
        } else if attribute_depth == 0 && bytes.get(i..i + 2) == Some(b"[|") {
            if let Some(island) = integer_array(bytes, i, min_elements.max(2), &cancelled) {
                i = island.end;
                islands.push(island);
            } else {
                i += 2;
            }
        } else {
            i += 1;
        }
    }
    if islands.is_empty() || attribute_depth != 0 {
        return None;
    }
    let mut projected = bytes.to_vec();
    for island in &islands {
        for byte in &mut projected[island.first_end..island.end - 2] {
            if !matches!(*byte, b'\n' | b'\r') {
                *byte = b' ';
            }
        }
    }
    Some(LiteralProjection {
        text: String::from_utf8(projected).expect("only ASCII literal interiors were replaced"),
        islands,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offsets_newlines_unicode_and_first_literal_are_preserved() {
        let source = "let ``😀`` = 1\nlet data = [| -1; 0;\n 1; 20 |]\nlet after = ``😀``\n";
        let p = project_integer_arrays(source, 3).unwrap();
        assert_eq!(source.len(), p.text.len());
        assert_eq!(
            source.match_indices('\n').map(|x| x.0).collect::<Vec<_>>(),
            p.text.match_indices('\n').map(|x| x.0).collect::<Vec<_>>()
        );
        assert_eq!(p.islands[0].element_count, 4);
        assert_eq!(
            &p.text[p.islands[0].first_start..p.islands[0].first_end],
            "-1"
        );
        assert_eq!(source.find("after"), p.text.find("after"));
        assert!(p.text.ends_with("let after = ``😀``\n"));
    }
    #[test]
    fn names_operators_suffixes_units_and_malformed_arrays_are_not_elided() {
        for body in [
            "1; named; 2",
            "- 1; - 2; 3",
            "1I; 2I; 3I",
            "1<m>; 2<m>; 3<m>",
            "1+2; 3; 4",
            "1 2; 3",
            ";1;2;3",
            "1;;2;3",
            "2147483648;1;2",
        ] {
            assert!(
                project_integer_arrays(&format!("let a = [|{body}|]"), 2).is_none(),
                "{body}"
            );
        }
        assert!(project_integer_arrays("let empty = [||]", 2).is_none());
    }
    #[test]
    fn strings_comments_and_escaped_identifiers_hide_array_delimiters() {
        let source="let a = \"[|1;2;3|]\"\nlet b = @\"[|1;2;3|]\"\nlet c = \"\"\"[|1;2;3|]\"\"\"\n// [|1;2;3|]\n(* nested (* [|1;2;3|] *) comment *)\nlet ``[|1;2;3|]`` = 0\nlet quote = '\"'\nlet real = [|1;2;3|]\n";
        let p = project_integer_arrays(source, 3).unwrap();
        assert_eq!(p.islands.len(), 1);
        assert_eq!(p.islands[0].start, source.rfind("[|").unwrap());
    }
    #[test]
    fn uncertain_lexical_states_decline_all_projection() {
        for extra in [
            "let s = $\"{x}\"",
            "#if X\n#endif",
            "(* unclosed",
            "let s=\"unclosed",
            "let ``unclosed = 1",
            "(* \"*)\" *)",
        ] {
            let source = format!("let data = [|1;2;3|]\n{extra}");
            assert!(project_integer_arrays(&source, 3).is_none(), "{extra}");
        }
        assert!(project_integer_arrays("[<Attr([|1;2;3|])>]\nlet x=1", 3).is_none());
    }
}
