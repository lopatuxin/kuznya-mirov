//! Служба сжатия картинок моделей: `importer watch <папка игр>` и `importer once <папка игр>`.

#[cfg(test)]
mod engine_compat;
mod game;
mod ktx2_writer;
mod model;
#[cfg(test)]
mod testing;
mod texture;
mod watch;

use std::path::Path;
use std::process::ExitCode;

fn print_line(line: &str) {
    println!("{line}");
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [command, root] if command == "once" => watch::once(Path::new(root), &print_line),
        [command, root] if command == "watch" => watch::watch(Path::new(root), &print_line),
        _ => {
            eprintln!("использование: importer watch <папка игр> | importer once <папка игр>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ошибка: {error:#}");
            ExitCode::FAILURE
        }
    }
}
