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
}

fn parse_pattern(pattern: &str) -> (Vec<PatternElement>, bool, bool) {
    let mut elements = Vec::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut start_anchored = false;
    if !chars.is_empty() && chars[0] == '^' {
        start_anchored = true;
        i = 1;
    }
    let mut end_anchored = false;
    while i < chars.len() {
        let mut base: Option<PatternElement> = None;
        let c = chars[i];
        if c == '\\' {
            i += 1;
            if i >= chars.len() {
                panic!("Invalid pattern: incomplete escape");
            }
            match chars[i] {
                '\\' => base = Some(PatternElement::Literal('\\')),
                'd' => base = Some(PatternElement::Digit),
                'w' => base = Some(PatternElement::Word),
                _ => panic!("Unhandled escape: \\{}", chars[i]),
            }
            i += 1;
        } else if c == '[' {
            i += 1;
            let mut neg = false;
            if i < chars.len() && chars[i] == '^' {
                neg = true;
                i += 1;
            }
            let mut inner = String::new();
            while i < chars.len() && chars[i] != ']' {
                inner.push(chars[i]);
                i += 1;
            }
            if i >= chars.len() || chars[i] != ']' {
                panic!("Unhandled pattern: unclosed group");
            }
            i += 1;
            base = Some(if neg {
                PatternElement::NegGroup(inner)
            } else {
                PatternElement::PosGroup(inner)
            });
        } else if c == '$' && i + 1 == chars.len() {
            end_anchored = true;
            i += 1;
        } else {
            if c == '.' {
                base = Some(PatternElement::Any);
            } else {
                base = Some(PatternElement::Literal(c));
            }
            i += 1;
        }
        if let Some(mut elem) = base {
            if i < chars.len() && chars[i] == '+' {
                i += 1;
                elem = PatternElement::OneOrMore(Box::new(elem));
            } else if i < chars.len() && chars[i] == '?' {
                i += 1;
                elem = PatternElement::Optional(Box::new(elem));
            }
            elements.push(elem);
        }
    }
    (elements, start_anchored, end_anchored)
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
        if let Some(end) = try_match_from(start, &elements, &input_chars) {
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

fn try_match_from(pos: usize, elems: &[PatternElement], input_chars: &[char]) -> Option<usize> {
    if elems.is_empty() {
        return Some(pos);
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
            ) -> Option<usize> {
                if let Some(after_one) = try_match_from(pos, &[inner.clone()], input_chars) {
                    if let Some(end) = match_one_or_more(after_one, inner, rest, input_chars) {
                        return Some(end);
                    }
                    try_match_from(after_one, rest, input_chars)
                } else {
                    None
                }
            }
            match_one_or_more(pos, inner, rest, input_chars)
        }
        PatternElement::Optional(inner) => {
            if let Some(new_pos) = try_match_from(pos, &[*inner.clone()], input_chars) {
                if let Some(end) = try_match_from(new_pos, rest, input_chars) {
                    return Some(end);
                }
            }
            try_match_from(pos, rest, input_chars)
        }
        PatternElement::Literal(l) => {
            if pos < input_chars.len() && input_chars[pos] == *l {
                try_match_from(pos + 1, rest, input_chars)
            } else {
                None
            }
        }
        PatternElement::Digit => {
            if pos < input_chars.len() && input_chars[pos].is_ascii_digit() {
                try_match_from(pos + 1, rest, input_chars)
            } else {
                None
            }
        }
        PatternElement::Word => {
            if pos < input_chars.len()
                && (input_chars[pos].is_ascii_alphanumeric() || input_chars[pos] == '_')
            {
                try_match_from(pos + 1, rest, input_chars)
            } else {
                None
            }
        }
        PatternElement::Any => {
            if pos < input_chars.len() {
                try_match_from(pos + 1, rest, input_chars)
            } else {
                None
            }
        }
        PatternElement::PosGroup(inner) => {
            if pos < input_chars.len() && inner.contains(input_chars[pos]) {
                try_match_from(pos + 1, rest, input_chars)
            } else {
                None
            }
        }
        PatternElement::NegGroup(inner) => {
            if pos < input_chars.len() && !inner.contains(input_chars[pos]) {
                try_match_from(pos + 1, rest, input_chars)
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
