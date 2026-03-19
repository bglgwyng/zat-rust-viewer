use clap::Parser;
use zat_rust_viewer::extract_outline;
use std::fs;

#[derive(Parser)]
#[command(name = "zat-rust-viewer")]
#[command(about = "Rust source outline viewer for zat")]
struct Args {
    /// File path
    file: String,
}

fn main() {
    let args = Args::parse();

    let content = fs::read_to_string(&args.file).expect("Failed to read file");
    let result = extract_outline(&content);

    for import in &result.imports {
        println!("{} // L{}", import.source_text, import.start_line);
    }

    if !result.imports.is_empty() && !result.exports.is_empty() {
        println!();
    }

    for entry in &result.exports {
        let mut lines = entry.signature.lines();
        if let Some(first) = lines.next() {
            println!("{} // L{}-L{}", first, entry.start_line, entry.end_line);
            for line in lines {
                println!("{}", line);
            }
        }
    }
}
