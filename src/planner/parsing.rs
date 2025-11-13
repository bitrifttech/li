pub(crate) fn extract_json_object(input: &str) -> Option<String> {
    let mut cleaned = input.to_string();

    loop {
        if let Some(think_start) = cleaned.find("<think>") {
            if let Some(think_end_pos) = cleaned[think_start..].find("</think>") {
                let absolute_end = think_start + think_end_pos + "</think>".len();
                cleaned.replace_range(think_start..absolute_end, "");
            } else {
                cleaned.replace_range(think_start.., "");
                break;
            }
        } else {
            break;
        }
    }

    let trimmed = cleaned.trim();
    let start = trimmed.find('{')?;

    let mut depth = 0;
    let mut end = None;
    for (idx, ch) in trimmed[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(start + idx);
                    break;
                }
            }
            _ => {}
        }
    }

    let end = end?;
    Some(trimmed[start..=end].to_string())
}
