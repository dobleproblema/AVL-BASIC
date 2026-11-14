use avl_basic::{console, Interpreter, RunOutcome};
use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let _ = console::install_ctrl_c_handler();
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    if let Some(status) = run_embedded() {
        std::process::exit(status);
    }
    let mut interpreter = Interpreter::new();
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        interpreter.enable_pack_export();
        std::process::exit(interpreter.repl());
    }

    let path = PathBuf::from(&args[0]);
    let code = run_program(&mut interpreter, &path, false);
    std::process::exit(code);
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn run_embedded() -> Option<i32> {
    let game = match avl_basic::pack::EmbeddedGame::open() {
        Ok(game) => game?,
        Err(error) => {
            report_pack_gui_error(&format!("PACK: {error}"));
            return Some(1);
        }
    };
    // Drop the interpreter's open files before deleting the extracted tree.
    let mut interpreter = Interpreter::new();
    let gui = game.is_gui();
    if gui {
        interpreter.enable_pack_gui();
    }
    let code = match game.prepare(&mut interpreter) {
        Ok(path) => run_program(&mut interpreter, &path, gui),
        Err(error) => {
            report_program_error(&format!("PACK: {error}"), gui);
            1
        }
    };
    Some(code)
}

fn run_program(interpreter: &mut Interpreter, path: &Path, gui: bool) -> i32 {
    interpreter.set_stream_output(true);
    let code = match interpreter
        .load_file(path)
        .and_then(|_| interpreter.run_loaded())
        .and_then(|outcome| {
            if outcome == RunOutcome::End {
                interpreter.wait_for_audio()?;
            }
            Ok(outcome)
        }) {
        Ok(_) => 0,
        Err(err) if gui && err.code == avl_basic::ErrorCode::KeyboardInterrupt => 0,
        Err(err) => {
            report_program_error(&err.display_diagnostic(), gui);
            1
        }
    };
    print!("{}", interpreter.take_output());
    code
}

fn report_program_error(error: &str, gui: bool) {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    if gui {
        report_pack_gui_error(error);
        return;
    }
    let _ = gui;
    eprintln!("{error}");
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn report_pack_gui_error(error: &str) {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleWindow() -> *mut c_void;
        fn GetStdHandle(kind: u32) -> *mut c_void;
    }
    #[link(name = "user32")]
    extern "system" {
        fn MessageBoxW(window: *mut c_void, text: *const u16, title: *const u16, style: u32)
            -> i32;
    }

    const STD_ERROR_HANDLE: u32 = (-12_i32) as u32;
    const MB_ICONERROR: u32 = 0x10;
    const MB_TASKMODAL: u32 = 0x2000;
    const MB_SETFOREGROUND: u32 = 0x10000;

    let stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
    if (!stderr.is_null() && stderr as isize != -1) || !unsafe { GetConsoleWindow() }.is_null() {
        eprintln!("{error}");
        return;
    }

    let message: Vec<u16> = error.encode_utf16().chain([0]).collect();
    let title: Vec<u16> = "AVL BASIC".encode_utf16().chain([0]).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR | MB_TASKMODAL | MB_SETFOREGROUND,
        );
    }
}
