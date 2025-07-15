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

fn parse_pattern(pattern: &str) -> Vec<PatternElement> {
    let mut elements = Vec::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
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
            elements.push(PatternElement::Literal(c));
        }
        i += 1;
    }
    elements
}

fn match_pattern(input_line: &str, pattern: &str) -> bool {
    let elements = parse_pattern(pattern);
    if elements.is_empty() {
        return true;
    }
    let input_chars: Vec<char> = input_line.chars().collect();
    let pat_len = elements.len();
    for start in 0..=input_chars.len().saturating_sub(pat_len) {
        let mut matched = true;
        for j in 0..pat_len {
            let ch = input_chars[start + j];
            let elem = &elements[j];
            let this_match = match elem {
                PatternElement::Literal(l) => ch == *l,
                PatternElement::Digit => ch.is_ascii_digit(),
                PatternElement::Word => ch.is_ascii_alphanumeric() || ch == '_',
                PatternElement::PosGroup(inner) => inner.contains(ch),
                PatternElement::NegGroup(inner) => !inner.contains(ch),
            };
            if !this_match {
                matched = false;
                break;
            }
        }
        if matched {
            return true;
        }
    }
    false
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

    if match_pattern(&input_line, &pattern) {
        process::exit(0)
    } else {
        process::exit(1)
    }
}
