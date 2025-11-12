use anyhow::{bail, Result};
use std::{
    env, fs,
    io::{self, Read},
    path::Path,
    process,
};

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
    Star(Box<Node>),
    Rep(Box<Node>, usize),
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
            } else if *i < cs.len() && cs[*i] == '*' {
                *i += 1;
                n = Node::Star(Box::new(n));
            } else if *i < cs.len() && cs[*i] == '{' {
                *i += 1;
                let mut num_str = String::new();
                while *i < cs.len() && cs[*i].is_ascii_digit() {
                    num_str.push(cs[*i]);
                    *i += 1;
                }
                if *i >= cs.len() || cs[*i] != '}' {
                    bail!("invalid repretition quantifier");
                }
                *i += 1;
                let count: usize = num_str.parse()?;
                n = Node::Rep(Box::new(n), count);
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

/// Prints a segment with optional filename prefix, preserving existing newline.
fn print_with_prefix(prefix: Option<&str>, seg: &str) {
    match prefix {
        Some(pfx) if seg.ends_with('\n') => print!("{}:{}", pfx, seg),
        Some(pfx) => print!("{}:{}\n", pfx, seg),
        None if seg.ends_with('\n') => print!("{}", seg),
        None => println!("{}", seg),
    }
}

/// Prints matching lines from content with optional prefix; returns true if any matched.
fn grep_content(content: &str, pattern: &str, prefix: Option<&str>) -> Result<bool> {
    let mut any = false;
    let mut consumed = 0usize;
    for seg in content.split_inclusive('\n') {
        let ln = seg.trim_end_matches(|c| c == '\n' || c == '\r');
        if is_match(ln, pattern)? {
            any = true;
            print_with_prefix(prefix, seg);
        }
        consumed += seg.len();
    }
    if consumed < content.len() {
        let seg = &content[consumed..];
        let ln = seg.trim_end_matches('\r');
        if is_match(ln, pattern)? {
            any = true;
            print_with_prefix(prefix, seg);
        }
    }
    Ok(any)
}

fn grep_file_with_label(path: &Path, pattern: &str, label: &str) -> Result<bool> {
    let content = fs::read_to_string(path)?;
    grep_content(&content, pattern, Some(label))
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

    match head {
        Node::Plus(inner) => more(pos, &*inner, tail, cs, caps),
        Node::Star(inner) => {
            if let Some((e, c)) = more(pos, &*inner, tail, cs, caps.clone()) {
                Some((e, c))
            } else {
                match_from(pos, tail, cs, caps)
            }
        }
        Node::Rep(inner, count) => {
            let mut p = pos;
            let mut c = caps;
            for _ in 0..*count {
                if let Some((np, nc)) = match_from(p, &[*(*inner).clone()], cs, c) {
                    p = np;
                    c = nc;
                } else {
                    return None;
                }
            }
            match_from(p, tail, cs, c)
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

/// Recursively searches a directory or file, labeling outputs relateive to procided root arguement
fn grep_dir(root: &str, pattern: &str) -> Result<bool> {
    let base = Path::new(root);
    let label_base = root.trim_end_matches(std::path::MAIN_SEPARATOR);
    fn walk(
        base: &Path,
        label_base: &str,
        dir: &Path,
        pattern: &str,
        any: &mut bool,
    ) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() {
                walk(base, label_base, &path, pattern, any)?;
            } else if ft.is_file() {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                let label = if rel.as_os_str().is_empty() {
                    label_base.to_string()
                } else {
                    format!("{}/{}", label_base, rel.display())
                };
                if grep_file_with_label(&path, pattern, &label)? {
                    *any = true;
                }
            }
        }
        Ok(())
    }
    let mut any = false;
    if base.is_dir() {
        walk(base, label_base, base, pattern, &mut any)?;
    } else if base.is_file() {
        let label = label_base.to_string();
        if grep_file_with_label(base, pattern, &label)? {
            any = true;
        }
    }
    Ok(any)
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
    let mut recursive = false;
    let mut head = args.next().unwrap_or_default();
    if head == "-r" {
        recursive = true;
        head = args.next().unwrap_or_default();
    }
    if head != "-E" {
        bail!("Expected '-E' after flags");
    }
    let pattern = args.next().unwrap_or_default();
    let rest: Vec<String> = args.collect();

    if recursive {
        if rest.is_empty() {
            return Ok(1);
        }
        let mut any = false;
        for root in &rest {
            if grep_dir(root, &pattern)? {
                any = true;
            }
        }
        return Ok(if any { 0 } else { 1 });
    }

    if rest.is_empty() {
        // stdin
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(if grep_content(&buf, &pattern, None)? {
            0
        } else {
            1
        })
    } else {
        let prefix = rest.len() > 1;
        let mut any = false;
        for file in &rest {
            if grep_file(file, &pattern, prefix)? {
                any = true;
            }
        }
        Ok(if any { 0 } else { 1 })
    }
}
