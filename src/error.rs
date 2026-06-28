use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    ImmediateCommand,
    NonImmediateCommand,
    Syntax,
    InvalidLineFormat,
    TypeMismatch,
    Undefined,
    DivisionByZero,
    Overflow,
    InvalidValue,
    InvalidName,
    InvalidLineNumber,
    InvalidArgument,
    InvalidTargetLine,
    TargetLineNotFound,
    ReturnWithoutGosub,
    ReturnNotFound,
    NextWithoutFor,
    ForWithoutNext,
    WendWithoutWhile,
    WhileWithoutWend,
    ResumeWithoutError,
    NumericExpression,
    EvalError,
    UnknownType,
    DataExhausted,
    NoData,
    NoDimension,
    IndexError,
    IndexOutOfRange,
    OutOfBounds,
    InvalidIndex,
    UndefinedIndex,
    InvalidDimensions,
    UsingFormat,
    VarNumberMismatch,
    FunctionError,
    ForbiddenExpression,
    MissingQuotes,
    FileNotFound,
    KeyboardInterrupt,
    OnlyBasFiles,
    OnlyPngFiles,
    HandlerError,
    MergeError,
    ArgumentMismatch,
    LocalNotAtStart,
    NoStoppedProgram,
    Unsupported,
    FunctionForbidden,
    FnEndWithoutDef,
    IfWithoutEndIf,
    ElseWithoutIf,
    EndIfWithoutIf,
    MatIdnDimension,
    SubroutineForbidden,
    SubEndWithoutDef,
}

impl ErrorCode {
    pub fn number(self) -> i32 {
        match self {
            ErrorCode::ImmediateCommand => 1,
            ErrorCode::NonImmediateCommand => 2,
            ErrorCode::InvalidLineFormat => 3,
            ErrorCode::InvalidValue => 4,
            ErrorCode::TypeMismatch => 5,
            ErrorCode::DivisionByZero => 6,
            ErrorCode::Overflow => 7,
            ErrorCode::Undefined => 8,
            ErrorCode::InvalidName => 9,
            ErrorCode::InvalidLineNumber => 10,
            ErrorCode::InvalidTargetLine => 11,
            ErrorCode::TargetLineNotFound => 12,
            ErrorCode::ReturnWithoutGosub => 13,
            ErrorCode::ReturnNotFound => 14,
            ErrorCode::Syntax => 15,
            ErrorCode::ForWithoutNext => 16,
            ErrorCode::NextWithoutFor => 17,
            ErrorCode::WhileWithoutWend => 18,
            ErrorCode::WendWithoutWhile => 19,
            ErrorCode::ResumeWithoutError => 20,
            ErrorCode::NumericExpression => 21,
            ErrorCode::EvalError => 22,
            ErrorCode::UnknownType => 23,
            ErrorCode::DataExhausted => 24,
            ErrorCode::NoData => 25,
            ErrorCode::InvalidArgument => 26,
            ErrorCode::NoDimension => 27,
            ErrorCode::IndexError => 28,
            ErrorCode::UsingFormat => 29,
            ErrorCode::VarNumberMismatch => 30,
            ErrorCode::ArgumentMismatch => 31,
            ErrorCode::InvalidIndex => 32,
            ErrorCode::UndefinedIndex => 33,
            ErrorCode::IndexOutOfRange => 34,
            ErrorCode::OutOfBounds => 35,
            ErrorCode::FunctionError => 36,
            ErrorCode::ForbiddenExpression => 37,
            ErrorCode::Unsupported => 38,
            ErrorCode::MissingQuotes => 39,
            ErrorCode::FileNotFound => 40,
            ErrorCode::KeyboardInterrupt => 41,
            ErrorCode::OnlyBasFiles => 42,
            ErrorCode::OnlyPngFiles => 43,
            ErrorCode::HandlerError => 44,
            ErrorCode::MergeError => 45,
            ErrorCode::NoStoppedProgram => 46,
            ErrorCode::FunctionForbidden => 47,
            ErrorCode::FnEndWithoutDef => 48,
            ErrorCode::IfWithoutEndIf => 49,
            ErrorCode::ElseWithoutIf => 50,
            ErrorCode::EndIfWithoutIf => 51,
            ErrorCode::InvalidDimensions => 52,
            ErrorCode::MatIdnDimension => 53,
            ErrorCode::SubroutineForbidden => 54,
            ErrorCode::SubEndWithoutDef => 55,
            ErrorCode::LocalNotAtStart => 56,
        }
    }

    pub fn from_number(number: i32) -> Option<Self> {
        Some(match number {
            1 => ErrorCode::ImmediateCommand,
            2 => ErrorCode::NonImmediateCommand,
            3 => ErrorCode::InvalidLineFormat,
            4 => ErrorCode::InvalidValue,
            5 => ErrorCode::TypeMismatch,
            6 => ErrorCode::DivisionByZero,
            7 => ErrorCode::Overflow,
            8 => ErrorCode::Undefined,
            9 => ErrorCode::InvalidName,
            10 => ErrorCode::InvalidLineNumber,
            11 => ErrorCode::InvalidTargetLine,
            12 => ErrorCode::TargetLineNotFound,
            13 => ErrorCode::ReturnWithoutGosub,
            14 => ErrorCode::ReturnNotFound,
            15 => ErrorCode::Syntax,
            16 => ErrorCode::ForWithoutNext,
            17 => ErrorCode::NextWithoutFor,
            18 => ErrorCode::WhileWithoutWend,
            19 => ErrorCode::WendWithoutWhile,
            20 => ErrorCode::ResumeWithoutError,
            21 => ErrorCode::NumericExpression,
            22 => ErrorCode::EvalError,
            23 => ErrorCode::UnknownType,
            24 => ErrorCode::DataExhausted,
            25 => ErrorCode::NoData,
            26 => ErrorCode::InvalidArgument,
            27 => ErrorCode::NoDimension,
            28 => ErrorCode::IndexError,
            29 => ErrorCode::UsingFormat,
            30 => ErrorCode::VarNumberMismatch,
            31 => ErrorCode::ArgumentMismatch,
            32 => ErrorCode::InvalidIndex,
            33 => ErrorCode::UndefinedIndex,
            34 => ErrorCode::IndexOutOfRange,
            35 => ErrorCode::OutOfBounds,
            36 => ErrorCode::FunctionError,
            37 => ErrorCode::ForbiddenExpression,
            38 => ErrorCode::Unsupported,
            39 => ErrorCode::MissingQuotes,
            40 => ErrorCode::FileNotFound,
            41 => ErrorCode::KeyboardInterrupt,
            42 => ErrorCode::OnlyBasFiles,
            43 => ErrorCode::OnlyPngFiles,
            44 => ErrorCode::HandlerError,
            45 => ErrorCode::MergeError,
            46 => ErrorCode::NoStoppedProgram,
            47 => ErrorCode::FunctionForbidden,
            48 => ErrorCode::FnEndWithoutDef,
            49 => ErrorCode::IfWithoutEndIf,
            50 => ErrorCode::ElseWithoutIf,
            51 => ErrorCode::EndIfWithoutIf,
            52 => ErrorCode::InvalidDimensions,
            53 => ErrorCode::MatIdnDimension,
            54 => ErrorCode::SubroutineForbidden,
            55 => ErrorCode::SubEndWithoutDef,
            56 => ErrorCode::LocalNotAtStart,
            _ => return None,
        })
    }

    pub fn message(self) -> &'static str {
        match self {
            ErrorCode::ImmediateCommand => "Instruction not allowed in a program.",
            ErrorCode::NonImmediateCommand => "Instruction not allowed in immediate mode.",
            ErrorCode::Syntax => "Syntax error.",
            ErrorCode::InvalidLineFormat => "Invalid line format.",
            ErrorCode::TypeMismatch => "Invalid value type.",
            ErrorCode::Undefined => "Undefined variable or function.",
            ErrorCode::DivisionByZero => "Division by zero.",
            ErrorCode::Overflow => "Numeric overflow.",
            ErrorCode::InvalidValue => "Invalid value.",
            ErrorCode::InvalidName => "Invalid name.",
            ErrorCode::InvalidLineNumber => "Invalid line number.",
            ErrorCode::InvalidArgument => "Invalid argument.",
            ErrorCode::InvalidTargetLine => "Invalid target line.",
            ErrorCode::TargetLineNotFound => "Target line does not exist.",
            ErrorCode::ReturnWithoutGosub => "RETURN without matching GOSUB.",
            ErrorCode::ReturnNotFound => "Invalid return line.",
            ErrorCode::NextWithoutFor => "NEXT without matching FOR.",
            ErrorCode::ForWithoutNext => "FOR without matching NEXT.",
            ErrorCode::WendWithoutWhile => "WEND without matching WHILE.",
            ErrorCode::WhileWithoutWend => "WHILE without matching WEND.",
            ErrorCode::ResumeWithoutError => "RESUME without ERROR.",
            ErrorCode::NumericExpression => "Expression must be numeric.",
            ErrorCode::EvalError => "Error evaluating expression.",
            ErrorCode::UnknownType => "Unknown data type.",
            ErrorCode::DataExhausted => "No more DATA to read.",
            ErrorCode::NoData => "The line has no DATA for RESTORE.",
            ErrorCode::NoDimension => "Empty dimension expression.",
            ErrorCode::IndexError => "Error generating array indices.",
            ErrorCode::IndexOutOfRange => "Index out of range.",
            ErrorCode::OutOfBounds => "Out of range.",
            ErrorCode::InvalidIndex => "Invalid index.",
            ErrorCode::UndefinedIndex => "Undefined index.",
            ErrorCode::InvalidDimensions => "Invalid number of dimensions.",
            ErrorCode::UsingFormat => "Error in PRINT USING format.",
            ErrorCode::VarNumberMismatch => "Number of inputs does not match number of variables.",
            ErrorCode::FunctionError => "Error evaluating function.",
            ErrorCode::ForbiddenExpression => "Expression not allowed.",
            ErrorCode::MissingQuotes => "Missing quotes.",
            ErrorCode::FileNotFound => "File not found.",
            ErrorCode::KeyboardInterrupt => "Execution interrupted by user.",
            ErrorCode::OnlyBasFiles => "Only names with extension '.bas' are allowed.",
            ErrorCode::OnlyPngFiles => "Only names with extension '.png' are allowed.",
            ErrorCode::HandlerError => "Error while handling errors.",
            ErrorCode::MergeError => "Error merging files.",
            ErrorCode::ArgumentMismatch => "Incorrect number of arguments.",
            ErrorCode::LocalNotAtStart => {
                "LOCAL must appear in the initial block of a function or subroutine."
            }
            ErrorCode::NoStoppedProgram => "There is no stopped program to continue.",
            ErrorCode::Unsupported => "Feature not supported in this environment.",
            ErrorCode::FunctionForbidden => "Instruction not allowed inside a function.",
            ErrorCode::FnEndWithoutDef => "Malformed function.",
            ErrorCode::IfWithoutEndIf => "IF without matching END IF.",
            ErrorCode::ElseWithoutIf => "ELSE without matching IF.",
            ErrorCode::EndIfWithoutIf => "END IF without matching IF.",
            ErrorCode::MatIdnDimension => "MAT IDN requires a two-dimensional square matrix.",
            ErrorCode::SubroutineForbidden => "Instruction not allowed inside a subroutine.",
            ErrorCode::SubEndWithoutDef => "Malformed subroutine.",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BasicError {
    pub code: ErrorCode,
    pub line: Option<i32>,
    pub detail: Option<String>,
}

impl BasicError {
    pub fn new(code: ErrorCode) -> Self {
        Self {
            code,
            line: None,
            detail: None,
        }
    }

    pub fn at_line(mut self, line: i32) -> Self {
        self.line = Some(line);
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn display_for_basic(&self) -> String {
        let message = self
            .detail
            .as_deref()
            .unwrap_or_else(|| self.code.message());
        match self.line {
            Some(line) => format!("Line {line}. {message}"),
            None => message.to_string(),
        }
    }
}

impl fmt::Display for BasicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_for_basic())
    }
}

impl std::error::Error for BasicError {}

pub type BasicResult<T> = Result<T, BasicError>;
