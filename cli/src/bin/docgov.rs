use std::process;

fn main() {
    match docgov::cli::run() {
        Ok(code) => process::exit(code),
        Err(err) => {
            eprintln!("Error: {:#}", err);
            process::exit(2);
        }
    }
}
