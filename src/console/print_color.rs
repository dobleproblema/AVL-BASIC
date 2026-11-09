use crate::graphics::resolve_color_number;
use std::io::{self, IsTerminal};
use std::sync::OnceLock;

pub(crate) const RESET_PRINT_COLOR: &str = "\x1b[39m";
pub(crate) const RESET_PRINT_BACKGROUND: &str = "\x1b[49m";
pub(crate) const RESET_PRINT_COLORS: &str = "\x1b[39;49m";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PrintColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
}

pub(crate) fn print_color_depth(ansi: bool) -> Option<PrintColorDepth> {
    static DEPTH: OnceLock<Option<PrintColorDepth>> = OnceLock::new();
    if !ansi || !io::stdout().is_terminal() {
        return None;
    }
    *DEPTH.get_or_init(|| {
        let term = std::env::var("TERM").unwrap_or_default();
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        let windows_terminal = cfg!(windows) && std::env::var_os("WT_SESSION").is_some();
        let depth = color_depth_from_environment(&term, &colorterm, windows_terminal)?;
        #[cfg(windows)]
        if !crossterm::ansi_support::supports_ansi() {
            return None;
        }
        Some(depth)
    })
}

fn color_depth_from_environment(
    term: &str,
    colorterm: &str,
    windows_terminal: bool,
) -> Option<PrintColorDepth> {
    let term = term.trim().to_ascii_lowercase();
    if matches!(term.as_str(), "dumb" | "vt52" | "vt100" | "xterm-mono") {
        return None;
    }

    let colorterm = colorterm.trim();
    if windows_terminal
        || colorterm.eq_ignore_ascii_case("truecolor")
        || colorterm.eq_ignore_ascii_case("24bit")
        || term.rsplit_once('-').is_some_and(|(_, suffix)| {
            matches!(
                suffix,
                "direct" | "direct2" | "direct16" | "direct256" | "truecolor" | "24bit"
            )
        })
    {
        Some(PrintColorDepth::TrueColor)
    } else if term.contains("256color")
        || matches!(term.as_str(), "alacritty" | "wezterm" | "xterm-kitty")
    {
        Some(PrintColorDepth::Ansi256)
    } else {
        Some(PrintColorDepth::Ansi16)
    }
}

impl PrintColorDepth {
    pub(crate) fn foreground(self, color: i32) -> String {
        let [_, r, g, b] = resolve_color_number(color).to_be_bytes();
        match self {
            Self::TrueColor => format!("\x1b[38;2;{r};{g};{b}m"),
            Self::Ansi256 => format!("\x1b[38;5;{}m", nearest_xterm_color([r, g, b])),
            Self::Ansi16 => format!("\x1b[{}m", nearest_ansi_foreground([r, g, b])),
        }
    }

    pub(crate) fn background(self, color: i32) -> String {
        let [_, r, g, b] = resolve_color_number(color).to_be_bytes();
        match self {
            Self::TrueColor => format!("\x1b[48;2;{r};{g};{b}m"),
            Self::Ansi256 => format!("\x1b[48;5;{}m", nearest_xterm_color([r, g, b])),
            Self::Ansi16 => format!("\x1b[{}m", nearest_ansi_foreground([r, g, b]) + 10),
        }
    }
}

fn nearest_xterm_color(rgb: [u8; 3]) -> usize {
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    const GRAY_START: i32 = 8;
    const GRAY_STEP: i32 = 10;

    let [r, g, b] = rgb.map(|channel| {
        (0..CUBE.len())
            .min_by_key(|&index| channel.abs_diff(CUBE[index]))
            .unwrap()
    });
    let cube_distance = distance_squared(rgb, [CUBE[r], CUBE[g], CUBE[b]]);

    // The nearest neutral gray minimizes squared distance at the channel mean.
    let sum: i32 = rgb.into_iter().map(i32::from).sum();
    let gray_index =
        ((sum - 3 * GRAY_START + 3 * GRAY_STEP / 2).div_euclid(3 * GRAY_STEP)).clamp(0, 23);
    let gray = (GRAY_START + gray_index * GRAY_STEP) as u8;
    if distance_squared(rgb, [gray; 3]) < cube_distance {
        232 + gray_index as usize
    } else {
        16 + 36 * r + 6 * g + b
    }
}

fn nearest_ansi_foreground(rgb: [u8; 3]) -> usize {
    const PALETTE: [[u8; 3]; 16] = [
        [0, 0, 0],
        [128, 0, 0],
        [0, 128, 0],
        [128, 128, 0],
        [0, 0, 128],
        [128, 0, 128],
        [0, 128, 128],
        [192, 192, 192],
        [128, 128, 128],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [0, 0, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
    ];
    let index = (0..PALETTE.len())
        .min_by_key(|&index| distance_squared(rgb, PALETTE[index]))
        .unwrap();
    if index < 8 {
        30 + index
    } else {
        90 + index - 8
    }
}

fn distance_squared(rgb: [u8; 3], target: [u8; 3]) -> i32 {
    rgb.into_iter()
        .zip(target)
        .map(|(a, b)| {
            let diff = i32::from(a) - i32::from(b);
            diff * diff
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_color_depth_uses_advertised_capabilities() {
        use PrintColorDepth::*;
        for term in ["dumb", "vt52", "vt100", "xterm-mono"] {
            assert_eq!(color_depth_from_environment(term, "truecolor", true), None);
        }
        assert_eq!(color_depth_from_environment("", "", true), Some(TrueColor));
        assert_eq!(
            color_depth_from_environment("xterm-256color", "truecolor", false),
            Some(TrueColor)
        );
        assert_eq!(
            color_depth_from_environment("xterm-direct", "", false),
            Some(TrueColor)
        );
        assert_eq!(
            color_depth_from_environment("xterm-direct256", "", false),
            Some(TrueColor)
        );
        assert_eq!(
            color_depth_from_environment("screen-256color", "", false),
            Some(Ansi256)
        );
        for term in ["alacritty", "wezterm", "xterm-kitty"] {
            assert_eq!(color_depth_from_environment(term, "", false), Some(Ansi256));
        }
        assert_eq!(
            color_depth_from_environment("linux", "", false),
            Some(Ansi16)
        );
        assert_eq!(print_color_depth(false), None);
    }

    #[test]
    fn terminal_foreground_uses_avl_palette_and_terminal_depth() {
        assert_eq!(
            PrintColorDepth::TrueColor.foreground(2),
            "\x1b[38;2;255;0;0m"
        );
        assert_eq!(
            PrintColorDepth::TrueColor.foreground(0x123456),
            "\x1b[38;2;18;52;86m"
        );
        assert_eq!(
            PrintColorDepth::Ansi256.foreground(0xff5f00),
            "\x1b[38;5;202m"
        );
        assert_eq!(
            PrintColorDepth::Ansi256.foreground(0x808080),
            "\x1b[38;5;244m"
        );
        assert_eq!(PrintColorDepth::Ansi16.foreground(2), "\x1b[91m");
        assert_eq!(PrintColorDepth::Ansi16.foreground(3), "\x1b[32m");
    }

    #[test]
    fn terminal_background_uses_avl_palette_and_terminal_depth() {
        assert_eq!(
            PrintColorDepth::TrueColor.background(2),
            "\x1b[48;2;255;0;0m"
        );
        assert_eq!(
            PrintColorDepth::TrueColor.background(0x123456),
            "\x1b[48;2;18;52;86m"
        );
        assert_eq!(
            PrintColorDepth::Ansi256.background(0xff5f00),
            "\x1b[48;5;202m"
        );
        assert_eq!(
            PrintColorDepth::Ansi256.background(0x808080),
            "\x1b[48;5;244m"
        );
        assert_eq!(PrintColorDepth::Ansi16.background(2), "\x1b[101m");
        assert_eq!(PrintColorDepth::Ansi16.background(3), "\x1b[42m");
    }
}
