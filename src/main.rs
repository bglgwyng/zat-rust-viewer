use zat_rust_viewer::extract_outline;
use std::io::{self, Read};

fn main() {
    let mut content = String::new();
    io::stdin().read_to_string(&mut content).expect("Failed to read stdin");

    let result = extract_outline(&content);

    for import in &result.imports {
        println!("{} // L{}", import.source_text, import.start_line);
    }

    if !result.imports.is_empty() && !result.exports.is_empty() {
        println!();
    }

    for entry in &result.exports {
        if entry.start_line == 0 || entry.signature.starts_with("  ") {
            println!("{}", entry.signature);
        } else if entry.start_line == entry.end_line {
            println!("{} // L{}", entry.signature, entry.start_line);
        } else {
            println!("{} // L{}-L{}", entry.signature, entry.start_line, entry.end_line);
        }
    }
}
