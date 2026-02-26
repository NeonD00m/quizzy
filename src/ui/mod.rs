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
