use std::env;
use std::io;
use std::process;

enum PatternElement {
    Literal(char),
    Digit,
    Word,
    PosGroup(String),
    NegGroup(String),
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
        let c = chars[i];
        if c == '\\' {
            i += 1;
            if i >= chars.len() {
                panic!("Invalid pattern: incomplete escape");
            }
            match chars[i] {
                '\\' => elements.push(PatternElement::Literal('\\')),
                'd' => elements.push(PatternElement::Digit),
                'w' => elements.push(PatternElement::Word),
                _ => panic!("Unhandled escape: \\{}", chars[i]),
            }
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
            if neg {
                elements.push(PatternElement::NegGroup(inner));
            } else {
                elements.push(PatternElement::PosGroup(inner));
            }
        } else {
            if c == '$' && i + 1 == chars.len() {
                end_anchored = true;
            } else {
                elements.push(PatternElement::Literal(c));
            }
        }
        i += 1;
    }
    (elements, start_anchored, end_anchored)
}

fn match_pattern(input_line: &str, pattern: &str) -> bool {
    let (elements, start_anchored, end_anchored) = parse_pattern(pattern);
    let input_chars: Vec<char> = input_line.chars().collect();
    let pat_len = elements.len();
    let input_len = input_chars.len();
    let possible_starts = match (start_anchored, end_anchored) {
        (true, true) => {
            if input_len == pat_len {
                vec![0]
            } else {
                vec![]
            }
        }
        (true, false) => {
            if input_len >= pat_len {
                vec![0]
            } else {
                vec![]
            }
        }
        (false, true) => {
            if input_len >= pat_len {
                vec![input_len - pat_len]
            } else {
                vec![]
            }
        }
        (false, false) => (0..=input_len.saturating_sub(pat_len)).collect(),
    };
    possible_starts.iter().any(|start| {
        (0..pat_len).all(|j| {
            let ch = input_chars[start + j];
            match &elements[j] {
                PatternElement::Literal(l) => ch == *l,
                PatternElement::Digit => ch.is_ascii_digit(),
                PatternElement::Word => ch.is_ascii_alphanumeric() || ch == '_',
                PatternElement::PosGroup(inner) => inner.contains(ch),
                PatternElement::NegGroup(inner) => !inner.contains(ch),
            }
        })
    })
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
