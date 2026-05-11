pub mod console;
pub mod error;
pub mod expr;
pub mod fonts;
pub mod graphics;
pub mod interpreter;
pub mod lexer;
pub mod program;
pub mod value;
pub mod window;

pub use error::{BasicError, BasicResult, ErrorCode};
pub use graphics::Graphics;
pub use interpreter::{Interpreter, RunOutcome};
pub use value::Value;
