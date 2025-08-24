use std::env;
use std::io;
use std::process;

#[derive(Clone)]
enum PatternElement {
    Literal(char),
    Digit,
    Word,
    Any,
    PosGroup(String),
    NegGroup(String),
    Optional(Box<PatternElement>),
    OneOrMore(Box<PatternElement>),
    Capture(usize, Vec<Vec<PatternElement>>),
    CaptureEnd(usize, usize),
    Backref(usize),
}

fn parse_elements(chars: &[char], i: &mut usize, group_id: &mut usize) -> Vec<PatternElement> {
    let mut elements = Vec::new();
    while *i < chars.len() {
        let base: Option<PatternElement>;
        let c = chars[*i];
        if c == '\\' {
            *i += 1;
            if *i >= chars.len() {
                panic!("Invalid pattern: incomplete escape");
            }
            match chars[*i] {
                '\\' => base = Some(PatternElement::Literal('\\')),
                'd' => base = Some(PatternElement::Digit),
                'w' => base = Some(PatternElement::Word),
                '^' => base = Some(PatternElement::Literal('^')),
                '$' => base = Some(PatternElement::Literal('$')),
                '(' => base = Some(PatternElement::Literal('(')),
                ')' => base = Some(PatternElement::Literal(')')),
                '|' => base = Some(PatternElement::Literal('|')),
                '[' => base = Some(PatternElement::Literal('[')),
                ']' => base = Some(PatternElement::Literal(']')),
                '.' => base = Some(PatternElement::Literal('.')),
                '+' => base = Some(PatternElement::Literal('+')),
                '?' => base = Some(PatternElement::Literal('?')),
                '1' => base = Some(PatternElement::Backref(1)),
                '2' => base = Some(PatternElement::Backref(2)),
                '3' => base = Some(PatternElement::Backref(3)),
                '4' => base = Some(PatternElement::Backref(4)),
                '5' => base = Some(PatternElement::Backref(5)),
                '6' => base = Some(PatternElement::Backref(6)),
                '7' => base = Some(PatternElement::Backref(7)),
                '8' => base = Some(PatternElement::Backref(8)),
                '9' => base = Some(PatternElement::Backref(9)),
                _ => panic!("Unhandled escape: \\{}", chars[*i]),
            }
            *i += 1;
        } else if c == '[' {
            *i += 1;
            let mut neg = false;
            if *i < chars.len() && chars[*i] == '^' {
                neg = true;
                *i += 1;
            }
            let mut inner = String::new();
            while *i < chars.len() && chars[*i] != ']' {
                inner.push(chars[*i]);
                *i += 1;
            }
            if *i >= chars.len() || chars[*i] != ']' {
                panic!("Unhandled pattern: unclosed group");
            }
            *i += 1;
            base = Some(if neg {
                PatternElement::NegGroup(inner)
            } else {
                PatternElement::PosGroup(inner)
            });
        } else if c == '(' {
            *i += 1;
            *group_id += 1;
            let id = *group_id;
            let mut inner_str = String::new();
            let mut depth = 0;
            while *i < chars.len() {
                let ch = chars[*i];
                *i += 1;
                if ch == '(' {
                    depth += 1;
                }
                if ch == ')' {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                inner_str.push(ch);
            }
            let branches = parse_alternatives(&inner_str, group_id);
            base = Some(PatternElement::Capture(id, branches));
        } else if c == '.' {
            base = Some(PatternElement::Any);
            *i += 1;
        } else {
            base = Some(PatternElement::Literal(c));
            *i += 1;
        }
        if let Some(mut elem) = base {
            if *i < chars.len() && chars[*i] == '+' {
                *i += 1;
                elem = PatternElement::OneOrMore(Box::new(elem));
            } else if *i < chars.len() && chars[*i] == '?' {
                *i += 1;
                elem = PatternElement::Optional(Box::new(elem));
            }
            elements.push(elem);
        }
    }
    elements
}

fn parse_pattern(pattern: &str) -> (Vec<PatternElement>, bool, bool) {
    let mut start_anchored = false;
    let mut end_anchored = false;
    let mut pat = pattern;
    if pat.starts_with('^') {
        start_anchored = true;
        pat = &pat[1..];
    }
    if pat.ends_with('$') && !pat.ends_with("\\$") {
        end_anchored = true;
        pat = &pat[0..pat.len() - 1];
    }
    let chars: Vec<char> = pat.chars().collect();
    let mut i: usize = 0;
    let mut group_id: usize = 0;
    let elements = parse_elements(&chars, &mut i, &mut group_id);
    (elements, start_anchored, end_anchored)
}

fn parse_alternatives(pattern: &str, group_id: &mut usize) -> Vec<Vec<PatternElement>> {
    let mut branches = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut depth = 0;
    while i < chars.len() {
        let c = chars[i];
        if depth == 0 && c == '|' {
            let branch_chars: Vec<char> = current.chars().collect();
            let mut branch_i = 0;
            branches.push(parse_elements(&branch_chars, &mut branch_i, group_id));
            current = String::new();
        } else {
            current.push(c);
            if c == '(' {
                depth += 1;
            }
            if c == ')' {
                depth -= 1;
            }
        }
        i += 1;
    }
    let branch_chars: Vec<char> = current.chars().collect();
    let mut branch_i = 0;
    branches.push(parse_elements(&branch_chars, &mut branch_i, group_id));
    branches
}

fn match_pattern(input_line: &str, pattern: &str) -> bool {
    let (elements, start_anchored, end_anchored) = parse_pattern(pattern);
    let input_chars: Vec<char> = input_line.chars().collect();
    let input_len = input_chars.len();
    let possible_starts: Vec<usize> = if start_anchored {
        vec![0]
    } else {
        (0..=input_len).collect()
    };
    possible_starts.iter().any(|&start| {
        if let Some((end, _)) = try_match_from(start, &elements, &input_chars, Vec::new()) {
            if end_anchored {
                end == input_len
            } else {
                true
            }
        } else {
            false
        }
    })
}

fn try_match_from(
    pos: usize,
    elems: &[PatternElement],
    input_chars: &[char],
    captures: Vec<Option<String>>,
) -> Option<(usize, Vec<Option<String>>)> {
    if elems.is_empty() {
        return Some((pos, captures));
    }
    let elem = &elems[0];
    let rest = &elems[1..];
    match elem {
        PatternElement::OneOrMore(ref inner) => {
            fn match_one_or_more(
                pos: usize,
                inner: &PatternElement,
                rest: &[PatternElement],
                input_chars: &[char],
                captures: Vec<Option<String>>,
            ) -> Option<(usize, Vec<Option<String>>)> {
                if let Some((after_one, cap_after)) =
                    try_match_from(pos, &[inner.clone()], input_chars, captures)
                {
                    if let Some((end, cap_end)) =
                        match_one_or_more(after_one, inner, rest, input_chars, cap_after.clone())
                    {
                        return Some((end, cap_end));
                    }
                    try_match_from(after_one, rest, input_chars, cap_after)
                } else {
                    None
                }
            }
            match_one_or_more(pos, inner, rest, input_chars, captures)
        }
        PatternElement::Optional(inner) => {
            if let Some((new_pos, cap_with)) =
                try_match_from(pos, &[*inner.clone()], input_chars, captures.clone())
            {
                if let Some((end, cap_end)) = try_match_from(new_pos, rest, input_chars, cap_with) {
                    return Some((end, cap_end));
                }
            }
            try_match_from(pos, rest, input_chars, captures)
        }
        PatternElement::Literal(l) => {
            if pos < input_chars.len() && input_chars[pos] == *l {
                try_match_from(pos + 1, rest, input_chars, captures)
            } else {
                None
            }
        }
        PatternElement::Digit => {
            if pos < input_chars.len() && input_chars[pos].is_ascii_digit() {
                try_match_from(pos + 1, rest, input_chars, captures)
            } else {
                None
            }
        }
        PatternElement::Word => {
            if pos < input_chars.len()
                && (input_chars[pos].is_ascii_alphanumeric() || input_chars[pos] == '_')
            {
                try_match_from(pos + 1, rest, input_chars, captures)
            } else {
                None
            }
        }
        PatternElement::Any => {
            if pos < input_chars.len() {
                try_match_from(pos + 1, rest, input_chars, captures)
            } else {
                None
            }
        }
        PatternElement::PosGroup(inner) => {
            if pos < input_chars.len() && inner.contains(input_chars[pos]) {
                try_match_from(pos + 1, rest, input_chars, captures)
            } else {
                None
            }
        }
        PatternElement::NegGroup(inner) => {
            if pos < input_chars.len() && !inner.contains(input_chars[pos]) {
                try_match_from(pos + 1, rest, input_chars, captures)
            } else {
                None
            }
        }
        PatternElement::Capture(id, ref branches) => {
            let slot = id - 1;
            for branch in branches {
                let mut combined: Vec<PatternElement> = branch.clone();
                combined.push(PatternElement::CaptureEnd(slot, pos));
                combined.extend_from_slice(rest);
                if let Some((end, caps)) =
                    try_match_from(pos, &combined, input_chars, captures.clone())
                {
                    return Some((end, caps));
                }
            }
            None
        }
        PatternElement::CaptureEnd(slot, start) => {
            let mut new_captures = captures.clone();
            if new_captures.len() <= *slot {
                new_captures.resize(*slot + 1, None);
            }
            let matched = input_chars[*start..pos].iter().collect::<String>();
            new_captures[*slot] = Some(matched);
            try_match_from(pos, rest, input_chars, new_captures)
        }
        PatternElement::Backref(n) => {
            if let Some(Some(ref s)) = captures.get(n - 1) {
                let s_chars: Vec<char> = s.chars().collect();
                let len = s_chars.len();
                if pos + len <= input_chars.len() && input_chars[pos..pos + len] == s_chars[..] {
                    try_match_from(pos + len, rest, input_chars, captures)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

fn main() {
    eprintln!("[Putao LOG] Start");

    if env::args().nth(1).unwrap() != "-E" {
        println!("Expected first argument to be '-E'");
        process::exit(1);
    }

    let pattern = env::args().nth(2).unwrap();
    let mut input_line = String::new();

    io::stdin().read_line(&mut input_line).unwrap();

    if input_line.ends_with('\n') {
        input_line.pop();
    }

    if match_pattern(&input_line, &pattern) {
        process::exit(0)
    } else {
        process::exit(1)
    }
}
