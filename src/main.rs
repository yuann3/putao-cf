use std::env;
use std::io;
use std::process;

fn match_pattern(input_line: &str, pattern: &str) -> bool {
    if pattern.chars().count() == 1 {
        return input_line.contains(pattern);
    } else if pattern == "\\d" {
        input_line.chars().any(|c| ('0'..='9').contains(&c))
    } else if pattern == "\\w" {
        input_line
            .chars()
            .any(|c| c.is_ascii_alphanumeric() || c == '_')
    } else {
        panic!("Unhandled pattern: {}", pattern)
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

    if match_pattern(&input_line, &pattern) {
        process::exit(0)
    } else {
        process::exit(1)
    }
}
