//! Asks the user to confirm a change before emx makes it.

use std::io::{self, BufRead, IsTerminal, Write};

use crate::error::Error;

/// Fails unless emx may change things: `yes` was given, or stdin is a
/// terminal that can answer a question.
pub fn ensure_answerable(yes: bool) -> Result<(), Error> {
    if yes || io::stdin().is_terminal() {
        Ok(())
    } else {
        Err(Error::NeedsYes)
    }
}

/// Asks `question` on stderr and reads the answer from stdin.
pub fn confirm(question: &str) -> Result<bool, Error> {
    ask(question, &mut io::stdin().lock(), &mut io::stderr())
        .map_err(|source| Error::Prompt { source })
}

/// Writes `question` with a default of no, and reads one line of `input`:
/// `y` or `yes`, in any case, is yes.
fn ask(question: &str, input: &mut impl BufRead, output: &mut impl Write) -> io::Result<bool> {
    write!(output, "{question} [y/N] ")?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(text: &str) -> bool {
        let mut output = Vec::new();
        let yes = ask("Switch?", &mut text.as_bytes(), &mut output).unwrap();
        assert_eq!(output, b"Switch? [y/N] ");
        yes
    }

    #[test]
    fn yes_means_yes() {
        for text in ["y\n", "Y\n", "yes\n", " YES \n"] {
            assert!(answer(text), "{text:?}");
        }
    }

    #[test]
    fn anything_else_means_no() {
        for text in ["\n", "n\n", "no\n", "yep\n", ""] {
            assert!(!answer(text), "{text:?}");
        }
    }
}
