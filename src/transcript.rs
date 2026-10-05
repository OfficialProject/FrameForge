use crate::model::TranscriptEntry;

pub fn parse_vtt(text: &str) -> Vec<TranscriptEntry> {
    let mut out = Vec::new();
    let mut current: Option<(f64, f64, String)> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() { flush(&mut out, &mut current); continue; }
        if line == "WEBVTT" || line.starts_with("NOTE") || line.starts_with("Kind:") || line.starts_with("Language:") { continue; }
        if let Some((start, end)) = timing(line) {
            flush(&mut out, &mut current);
            current = Some((start, end, String::new()));
        } else if current.is_some() && !line.chars().all(|c| c.is_ascii_digit()) {
            let cleaned = strip_tags(line);
            if !cleaned.is_empty() {
                if let Some((_, _, ref mut text)) = current {
                    if !text.is_empty() { text.push(' '); }
                    text.push_str(&cleaned);
                }
            }
        }
    }
    flush(&mut out, &mut current);
    collapse(out)
}

fn flush(out: &mut Vec<TranscriptEntry>, current: &mut Option<(f64, f64, String)>) {
    if let Some((start, end, text)) = current.take() {
        if !text.is_empty() { out.push(TranscriptEntry { start, end, text }); }
    }
}

fn timing(line: &str) -> Option<(f64, f64)> {
    let mut parts = line.split(" --> ");
    Some((parse_time(parts.next()?)?, parse_time(parts.next()?.split_whitespace().next()?)?))
}

fn parse_time(value: &str) -> Option<f64> {
    let parts: Vec<_> = value.split(':').collect();
    match parts.as_slice() {
        [minutes, seconds] => Some(minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?),
        [hours, minutes, seconds] => Some(hours.parse::<f64>().ok()? * 3600.0 + minutes.parse::<f64>().ok()? * 60.0 + seconds.parse::<f64>().ok()?),
        _ => None,
    }
}

fn strip_tags(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut inside = false;
    for ch in value.chars() {
        match ch { '<' => inside = true, '>' => inside = false, _ if !inside => output.push(ch), _ => {} }
    }
    output.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&#39;", "'").replace("&quot;", "\"")
}

fn collapse(entries: Vec<TranscriptEntry>) -> Vec<TranscriptEntry> {
    let mut out = Vec::new();
    let mut previous = String::new();
    for mut entry in entries {
        let text = entry.text.trim().to_string();
        if text.is_empty() { continue; }
        let previous_chars: Vec<char> = previous.chars().collect();
        let text_chars: Vec<char> = text.chars().collect();
        let mut overlap = 0usize;
        for n in (1..=previous_chars.len().min(text_chars.len())).rev() {
            if n < text_chars.len() && text_chars[n] != ' ' { continue; }
            let cut = previous_chars.len() - n;
            if cut > 0 && previous_chars[cut - 1] != ' ' { continue; }
            if previous_chars[cut..] == text_chars[..n] { overlap = n; break; }
        }
        entry.text = text_chars[overlap..].iter().collect::<String>().trim().to_string();
        previous = text;
        if !entry.text.is_empty() { out.push(entry); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse_vtt;
    #[test]
    fn parses_and_collapses() {
        let input = "WEBVTT

00:00:00.000 --> 00:00:02.000
Hello world

00:00:01.000 --> 00:00:03.000
world this is CS2
";
        let result = parse_vtt(input);
        assert_eq!(result[0].text, "Hello world");
        assert_eq!(result[1].text, "this is CS2");
    }
    #[test]
    fn rejects_non_finite_and_reversed_timestamps() {
        assert!(parse_vtt("WEBVTT\n\nNaN --> 2.0\nignored\n").is_empty());
        assert!(parse_vtt("WEBVTT\n\n3.0 --> 2.0\nignored\n").is_empty());
    }
    #[test]
    fn unicode_overlap_is_safe() {
        let input = "WEBVTT\n\n00:00:00.000 --> 00:00:02.000\nHello 🌎\n\n00:00:01.000 --> 00:00:03.000\n🌎 world\n";
        let result = parse_vtt(input);
        assert_eq!(result[1].text, "world");
    }
}