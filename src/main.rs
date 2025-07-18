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
    Capture(Vec<Vec<PatternElement>>),
    Backref,
}

fn parse_elements(chars: &[char], i: &mut usize) -> Vec<PatternElement> {
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
                '1' => base = Some(PatternElement::Backref),
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
            let branches = parse_alternatives(&inner_str);
            base = Some(PatternElement::Capture(branches));
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
    let elements = parse_elements(&chars, &mut i);
    (elements, start_anchored, end_anchored)
}

fn parse_alternatives(pattern: &str) -> Vec<Vec<PatternElement>> {
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
            branches.push(parse_elements(&branch_chars, &mut branch_i));
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
    branches.push(parse_elements(&branch_chars, &mut branch_i));
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
        if let Some((end, _)) = try_match_from(start, &elements, &input_chars, None) {
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
    capture: Option<String>,
) -> Option<(usize, Option<String>)> {
    if elems.is_empty() {
        return Some((pos, capture));
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
                capture: Option<String>,
            ) -> Option<(usize, Option<String>)> {
                if let Some((after_one, cap_after)) =
                    try_match_from(pos, &[inner.clone()], input_chars, capture)
                {
                    if let Some((end, cap_end)) = match_one_or_more(
                        after_one,
                        inner,
                        rest,
                        input_chars,
                        cap_after.clone(),
                    ) {
                        return Some((end, cap_end));
                    }
                    try_match_from(after_one, rest, input_chars, cap_after)
                } else {
                    None
                }
            }
            match_one_or_more(pos, inner, rest, input_chars, capture)
        }
        PatternElement::Optional(inner) => {
            if let Some((new_pos, cap_with)) =
                try_match_from(pos, &[*inner.clone()], input_chars, capture.clone())
            {
                if let Some((end, cap_end)) =
                    try_match_from(new_pos, rest, input_chars, cap_with)
                {
                    return Some((end, cap_end));
                }
            }
            try_match_from(pos, rest, input_chars, capture)
        }
        PatternElement::Literal(l) => {
            if pos < input_chars.len() && input_chars[pos] == *l {
                try_match_from(pos + 1, rest, input_chars, capture)
            } else {
                None
            }
        }
        PatternElement::Digit => {
            if pos < input_chars.len() && input_chars[pos].is_ascii_digit() {
                try_match_from(pos + 1, rest, input_chars, capture)
            } else {
                None
            }
        }
        PatternElement::Word => {
            if pos < input_chars.len()
                && (input_chars[pos].is_ascii_alphanumeric() || input_chars[pos] == '_')
            {
                try_match_from(pos + 1, rest, input_chars, capture)
            } else {
                None
            }
        }
        PatternElement::Any => {
            if pos < input_chars.len() {
                try_match_from(pos + 1, rest, input_chars, capture)
            } else {
                None
            }
        }
        PatternElement::PosGroup(inner) => {
            if pos < input_chars.len() && inner.contains(input_chars[pos]) {
                try_match_from(pos + 1, rest, input_chars, capture)
            } else {
                None
            }
        }
        PatternElement::NegGroup(inner) => {
            if pos < input_chars.len() && !inner.contains(input_chars[pos]) {
                try_match_from(pos + 1, rest, input_chars, capture)
            } else {
                None
            }
        }
        PatternElement::Capture(ref branches) => {
            for branch in branches {
                if let Some((new_pos, _)) =
                    try_match_from(pos, branch, input_chars, capture.clone())
                {
                    let matched = input_chars[pos..new_pos].iter().collect::<String>();
                    if let Some((end, cap_end)) =
                        try_match_from(new_pos, rest, input_chars, Some(matched))
                    {
                        return Some((end, cap_end));
                    }
                }
            }
            None
        }
        PatternElement::Backref => {
            if let Some(ref s) = capture {
                let s_chars: Vec<char> = s.chars().collect();
                let len = s_chars.len();
                if pos + len <= input_chars.len() && input_chars[pos..pos + len] == s_chars[..]
                {
                    try_match_from(pos + len, rest, input_chars, capture)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}

//  echo <input_text> | cargo run -E <pattern>
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
