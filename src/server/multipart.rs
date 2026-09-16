pub struct FormDataPart {
    pub name: String,
    pub filename: Option<String>,
    pub data: Vec<u8>,
}

pub fn extract_boundary(content_type: &str) -> Option<String> {
    for part in content_type.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix("boundary=") {
            let b = rest.trim().trim_matches('"');
            return Some(b.to_string());
        }
    }
    None
}

/// Parses a multipart/form-data body in memory.
pub fn parse_multipart(content_type: &str, body: &[u8]) -> Result<Vec<FormDataPart>, String> {
    let boundary_str = extract_boundary(content_type).ok_or_else(|| "Missing boundary in Content-Type".to_string())?;

    let boundary = format!("--{}", boundary_str).into_bytes();

    let mut parts = Vec::new();
    let mut pos = 0;

    let start_idx = find_subsequence(&body[pos..], &boundary).ok_or_else(|| "Initial boundary not found".to_string())?;
    pos += start_idx + boundary.len();

    while pos < body.len() {
        if body[pos..].starts_with(b"\r\n") {
            pos += 2;
        } else if body[pos..].starts_with(b"--") {
            break;
        }

        let next_boundary_pos = match find_subsequence(&body[pos..], &boundary) {
            Some(idx) => pos + idx,
            None => break,
        };

        let mut part_bytes = &body[pos..next_boundary_pos];
        if part_bytes.ends_with(b"\r\n") {
            part_bytes = &part_bytes[..part_bytes.len() - 2];
        }

        if let Some(header_end) = find_subsequence(part_bytes, b"\r\n\r\n") {
            let header_bytes = &part_bytes[..header_end];
            let data_bytes = &part_bytes[header_end + 4..];

            let headers_str = String::from_utf8_lossy(header_bytes);
            let mut name = String::new();
            let mut filename = None;

            for line in headers_str.lines() {
                if let Some((header_name, header_val)) = line.split_once(':') {
                    if header_name.trim().eq_ignore_ascii_case("content-disposition") {
                        for param in header_val.split(';') {
                            let p = param.trim();
                            if let Some(n) = p.strip_prefix("name=") {
                                name = n.trim().trim_matches('"').to_string();
                            } else if let Some(f) = p.strip_prefix("filename=") {
                                filename = Some(f.trim().trim_matches('"').to_string());
                            }
                        }
                    }
                }
            }

            if !name.is_empty() {
                parts.push(FormDataPart { name, filename, data: data_bytes.to_vec() });
            }
        }

        pos = next_boundary_pos + boundary.len();
    }

    Ok(parts)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_multipart() {
        let content_type = "multipart/form-data; boundary=\"test-boundary\"";
        let body = b"--test-boundary\r\n\
Content-Disposition: form-data; name=\"score\"\r\n\r\n\
encrypted_score_data\r\n\
--test-boundary\r\n\
Content-Disposition: form-data; name=\"score\"; filename=\"replay.osr\"\r\n\r\n\
replay_binary_frames\r\n\
--test-boundary--\r\n";

        let parts = parse_multipart(content_type, body).unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].name, "score");
        assert_eq!(parts[0].filename, None);
        assert_eq!(parts[0].data, b"encrypted_score_data");

        assert_eq!(parts[1].name, "score");
        assert_eq!(parts[1].filename, Some("replay.osr".to_string()));
        assert_eq!(parts[1].data, b"replay_binary_frames");
    }
}
