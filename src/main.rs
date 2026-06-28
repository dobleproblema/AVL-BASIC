use avl_basic::{console, Interpreter};
use std::env;
use std::path::PathBuf;

fn main() {
    let _ = console::install_ctrl_c_handler();
    let mut interpreter = Interpreter::new();
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        std::process::exit(interpreter.repl());
    }

    let path = PathBuf::from(&args[0]);
    interpreter.set_stream_output(true);
    let code = match interpreter
        .load_file(&path)
        .and_then(|_| interpreter.run_loaded())
    {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err.display_for_basic());
            1
        }
    };
    print!("{}", interpreter.take_output());
    std::process::exit(code);
}
