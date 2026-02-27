pub mod cards;
pub mod gamble;
pub mod input;
pub mod learn;
pub mod stats;

pub fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()]; // what the helliante
    }

    let mut lines = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current_line = String::new();
        for word in line.split_whitespace() {
            let word_len = word.chars().count();

            // Check if word fits in the remainder of the current line
            let space_needed = if current_line.is_empty() { 0 } else { 1 };
            if current_line.chars().count() + space_needed + word_len <= max_width {
                if space_needed > 0 {
                    current_line.push(' ');
                }
                current_line.push_str(word);
            } else {
                // Word doesn't fit or it's too long for a whole line
                if !current_line.is_empty() {
                    lines.push(current_line);
                    current_line = String::new();
                }

                if word_len <= max_width {
                    current_line.push_str(word);
                } else {
                    // Hard wrap the long word
                    let mut start = 0;
                    let chars: Vec<char> = word.chars().collect();
                    while start < chars.len() {
                        let end = std::cmp::min(start + max_width, chars.len());
                        let chunk: String = chars[start..end].iter().collect();
                        if end < chars.len() {
                            lines.push(chunk);
                        } else {
                            current_line = chunk;
                        }
                        start = end;
                    }
                }
            }
        }
        if !current_line.is_empty() {
            lines.push(current_line);
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Prints two strings on the same line(s), with 'left' left-aligned and 'right' right-aligned.
/// If 'left' is too long, it wraps to subsequent lines while 'right' stays pinned to the first line's edge.
pub fn print_split_aligned(left: &str, right: &str, max_width: Option<usize>) {
    let (term_w, _) = crossterm::terminal::size().unwrap_or((80, 24));
    let width = max_width.unwrap_or(term_w as usize);

    let right_len = right.chars().count();
    // Ensure we have at least some space for the left side, with a small gutter
    let left_max_width = width.saturating_sub(right_len + 2).max(1);

    let left_lines = wrap_text(left, left_max_width);

    for (i, line) in left_lines.iter().enumerate() {
        if i == 0 {
            // First line: Print left text, pad with spaces, then print right text
            let padding = width.saturating_sub(line.chars().count() + right_len);
            println!("{}{: <padding$}{}", line, "", right, padding = padding);
        } else {
            // Subsequent lines: Just print the wrapped left text
            println!("{}", line);
        }
    }
}
