use std::{fmt, io::IsTerminal};

pub(crate) fn info(message: fmt::Arguments<'_>) {
    print(message, 32);
}

pub(crate) fn warn(message: fmt::Arguments<'_>) {
    print(format_args!("⚠️  {message}"), 33);
}

pub(crate) fn error(message: fmt::Arguments<'_>) {
    print(message, 31);
}

fn print(message: fmt::Arguments<'_>, color: u8) {
    let enabled = color_enabled(
        std::io::stderr().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var_os("TERM").is_some_and(|term| term == "dumb"),
    );
    eprintln!("{}", styled(message, color, enabled));
}

fn color_enabled(terminal: bool, no_color: bool, dumb: bool) -> bool {
    terminal && !no_color && !dumb
}

fn styled(message: fmt::Arguments<'_>, color: u8, enabled: bool) -> String {
    if enabled {
        format!("\x1b[{color}m{message}\x1b[0m")
    } else {
        message.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_require_a_terminal_and_respect_opt_outs() {
        assert!(color_enabled(true, false, false));
        assert!(!color_enabled(false, false, false));
        assert!(!color_enabled(true, true, false));
        assert!(!color_enabled(true, false, true));
    }

    #[test]
    fn messages_have_scoped_colors_or_plain_text() {
        for color in [31, 32, 33] {
            assert_eq!(
                styled(format_args!("message"), color, true),
                format!("\x1b[{color}mmessage\x1b[0m")
            );
            assert_eq!(styled(format_args!("message"), color, false), "message");
        }
    }
}
