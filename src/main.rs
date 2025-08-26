use anyhow::{bail, Result};
use std::{env, fs, io, process};

#[derive(Clone)]
enum Node {
    Lit(char),
    Digit,
    Word,
    Any,
    Pos(String),
    Neg(String),
    Opt(Box<Node>),
    Plus(Box<Node>),
    Cap(usize, Vec<Vec<Node>>),
    CapEnd(usize, usize),
    Ref(usize),
}

/// Parses a pattern into AST nodes and anchor flags.
fn parse(pattern: &str) -> Result<(Vec<Node>, bool, bool)> {
    let (mut start, mut end) = (false, false);
    let mut pat = pattern;
    if pat.starts_with('^') {
        start = true;
        pat = &pat[1..];
    }
    if pat.ends_with('$') && !pat.ends_with("\\$") {
        end = true;
        pat = &pat[..pat.len() - 1];
    }
    let cs: Vec<char> = pat.chars().collect();
    let mut i = 0usize;
    let mut gid = 0usize;
    Ok((elems(&cs, &mut i, &mut gid)?, start, end))
}

/// Parses a sequence of nodes until end or ')'.
fn elems(cs: &[char], i: &mut usize, gid: &mut usize) -> Result<Vec<Node>> {
    let mut out = Vec::new();
    while *i < cs.len() {
        let c = cs[*i];
        let base = if c == '\\' {
            *i += 1;
            if *i >= cs.len() {
                bail!("invalid escape");
            }
            let e = cs[*i];
            *i += 1;
            match e {
                'd' => Some(Node::Digit),
                'w' => Some(Node::Word),
                '1'..='9' => Some(Node::Ref((e as u8 - b'0') as usize)),
                _ => Some(Node::Lit(e)),
            }
        } else if c == '[' {
            *i += 1;
            let neg = *i < cs.len() && cs[*i] == '^';
            if neg {
                *i += 1;
            }
            let mut s = String::new();
            while *i < cs.len() && cs[*i] != ']' {
                s.push(cs[*i]);
                *i += 1;
            }
            if *i >= cs.len() || cs[*i] != ']' {
                bail!("unclosed class");
            }
            *i += 1;
            Some(if neg { Node::Neg(s) } else { Node::Pos(s) })
        } else if c == '(' {
            *i += 1;
            *gid += 1;
            let id = *gid;
            let mut buf = String::new();
            let mut d = 0;
            while *i < cs.len() {
                let ch = cs[*i];
                *i += 1;
                if ch == '(' {
                    d += 1;
                }
                if ch == ')' {
                    if d == 0 {
                        break;
                    }
                    d -= 1;
                }
                buf.push(ch);
            }
            Some(Node::Cap(id, branches(&buf, gid)?))
        } else if c == ')' {
            break;
        } else if c == '.' {
            *i += 1;
            Some(Node::Any)
        } else {
            *i += 1;
            Some(Node::Lit(c))
        };
        if let Some(mut n) = base {
            if *i < cs.len() && cs[*i] == '+' {
                *i += 1;
                n = Node::Plus(Box::new(n));
            } else if *i < cs.len() && cs[*i] == '?' {
                *i += 1;
                n = Node::Opt(Box::new(n));
            }
            out.push(n);
        }
    }
    Ok(out)
}

/// Parses top-level alternation branches within a group.
fn branches(s: &str, gid: &mut usize) -> Result<Vec<Vec<Node>>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    let mut d = 0i32;
    while i < cs.len() {
        let c = cs[i];
        if d == 0 && c == '|' {
            let v: Vec<char> = cur.chars().collect();
            let mut j = 0usize;
            out.push(elems(&v, &mut j, gid)?);
            cur.clear();
        } else {
            if c == '(' {
                d += 1;
            }
            if c == ')' {
                d -= 1;
            }
            cur.push(c);
        }
        i += 1;
    }
    let v: Vec<char> = cur.chars().collect();
    let mut j = 0usize;
    out.push(elems(&v, &mut j, gid)?);
    Ok(out)
}

/// Attempts to match the pattern against the input string.
fn is_match(input: &str, pat: &str) -> Result<bool> {
    let (nodes, start, end) = parse(pat)?;
    let cs: Vec<char> = input.chars().collect();
    let n = cs.len();
    let starts: Vec<usize> = if start { vec![0] } else { (0..=n).collect() };
    Ok(starts.iter().any(|&st| {
        match_from(st, &nodes, &cs, Vec::new())
            .map(|(e, _)| if end { e == n } else { true })
            .unwrap_or(false)
    }))
}

/// Prints amtching lines from content, preserving original line
/// ending and using and optional prefix
fn grep_content(content: &str, pattern: &str, prefix: Option<&str>) -> Result<bool> {
    let mut any = false;
    let mut consumed = 0usize;
    for seg in content.split_inclusive('\n') {
        let ln = seg.trim_end_matches(|c| c == '\n' || c == '\r');
        if is_match(ln, pattern)? {
            if let Some(pfx) = prefix {
                print!("{}:{}", pfx, seg);
            } else {
                print!("{}", seg);
            }
            any = true;
        }
        consumed += seg.len();
    }
    if consumed < content.len() {
        let last = &content[consumed..];
        let ln = last.trim_end_matches('\r');
        if is_match(ln, pattern)? {
            if let Some(pfx) = prefix {
                print!("{}:{}", pfx, last);
            } else {
                print!("{}", last);
            }
            any = true;
        }
    }
    Ok(any)
}

/// Backtracking matcher for a sequence of nodes from a position.
fn match_from(
    pos: usize,
    nodes: &[Node],
    cs: &[char],
    caps: Vec<Option<String>>,
) -> Option<(usize, Vec<Option<String>>)> {
    if nodes.is_empty() {
        return Some((pos, caps));
    }
    let head = &nodes[0];
    let tail = &nodes[1..];
    match head {
        Node::Plus(inner) => {
            fn more(
                pos: usize,
                inner: &Node,
                rest: &[Node],
                cs: &[char],
                caps: Vec<Option<String>>,
            ) -> Option<(usize, Vec<Option<String>>)> {
                if let Some((p1, c1)) = match_from(pos, &[inner.clone()], cs, caps) {
                    if let Some((e, c2)) = more(p1, inner, rest, cs, c1.clone()) {
                        return Some((e, c2));
                    }
                    match_from(p1, rest, cs, c1)
                } else {
                    None
                }
            }
            more(pos, &*inner, tail, cs, caps)
        }
        Node::Opt(inner) => {
            if let Some((p1, c1)) = match_from(pos, &[(*inner.clone())], cs, caps.clone()) {
                if let Some((e, c2)) = match_from(p1, tail, cs, c1) {
                    return Some((e, c2));
                }
            }
            match_from(pos, tail, cs, caps)
        }
        Node::Lit(ch) => {
            if pos < cs.len() && cs[pos] == *ch {
                match_from(pos + 1, tail, cs, caps)
            } else {
                None
            }
        }
        Node::Digit => {
            if pos < cs.len() && cs[pos].is_ascii_digit() {
                match_from(pos + 1, tail, cs, caps)
            } else {
                None
            }
        }
        Node::Word => {
            if pos < cs.len() && (cs[pos].is_ascii_alphanumeric() || cs[pos] == '_') {
                match_from(pos + 1, tail, cs, caps)
            } else {
                None
            }
        }
        Node::Any => {
            if pos < cs.len() {
                match_from(pos + 1, tail, cs, caps)
            } else {
                None
            }
        }
        Node::Pos(s) => {
            if pos < cs.len() && s.contains(cs[pos]) {
                match_from(pos + 1, tail, cs, caps)
            } else {
                None
            }
        }
        Node::Neg(s) => {
            if pos < cs.len() && !s.contains(cs[pos]) {
                match_from(pos + 1, tail, cs, caps)
            } else {
                None
            }
        }
        Node::Cap(id, brs) => {
            let slot = id - 1;
            for b in brs {
                let mut seq = b.clone();
                seq.push(Node::CapEnd(slot, pos));
                seq.extend_from_slice(tail);
                if let Some((e, c)) = match_from(pos, &seq, cs, caps.clone()) {
                    return Some((e, c));
                }
            }
            None
        }
        Node::CapEnd(slot, start) => {
            let mut nc = caps.clone();
            if nc.len() <= *slot {
                nc.resize(*slot + 1, None);
            }
            let s: String = cs[*start..pos].iter().collect();
            nc[*slot] = Some(s);
            match_from(pos, tail, cs, nc)
        }
        Node::Ref(n) => {
            if let Some(Some(s)) = caps.get(n - 1) {
                let rs: Vec<char> = s.chars().collect();
                let len = rs.len();
                if pos + len <= cs.len() && cs[pos..pos + len] == rs[..] {
                    match_from(pos + len, tail, cs, caps)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

/// CLI entrypoint compatible with the runner contract.
fn main() {
    match cli() {
        Ok(code) => process::exit(code),
        Err(e) => {
            eprintln!("{}", e);
            process::exit(1);
        }
    }
}

/// Reads a file and prints matches with optional filename prefixes.
fn grep_file(file: &str, pattern: &str, prefix: bool) -> Result<bool> {
    let content = fs::read_to_string(file)?;
    grep_content(&content, pattern, if prefix { Some(file) } else { None })
}

/// Parses args, matches against stdin or files, prints matches with optional
/// prefixes, return 0 on any match
fn cli() -> Result<i32> {
    let mut args = env::args();
    args.next();
    let arg1 = args.next().unwrap_or_default();
    if arg1 != "-E" {
        bail!("Expected first argument to be '-E'");
    }
    let pattern = args.next().unwrap_or_default();
    let files: Vec<String> = args.collect();

    if files.is_empty() {
        // stdin
        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        Ok(if is_match(&line, &pattern)? { 0 } else { 1 })
    } else {
        let prefix = files.len() > 1;
        let mut any = false;
        for file in &files {
            if grep_file(file, &pattern, prefix)? {
                any = true;
            }
        }
        Ok(if any { 0 } else { 1 })
    }
}
