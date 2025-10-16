use colored::*;

pub fn log(msg: &str) {
    println!("{} {}", "•".bright_black(), msg);
}

pub fn ok(msg: &str) {
    println!("{} {}", "✓".green(), msg.bold());
}

#[allow(dead_code)]
pub fn warn(msg: &str) {
    println!("{} {}", "!".yellow(), msg);
}

#[allow(dead_code)]
pub fn err(msg: &str) {
    eprintln!("{} {}", "x".red(), msg);
}
