use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, Read, Write};

const DEFAULT_MCP_PROTOCOL_VERSION: &str = "2024-11-05";
pub(super) const MAX_MCP_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_MCP_HEADER_BYTES: usize = 8 * 1024;

pub(super) struct McpMessage {
    pub(super) body: Vec<u8>,
    pub(super) framing: StdioFraming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StdioFraming {
    ContentLength,
    NewlineDelimited,
}

pub(super) fn read_message(reader: &mut impl BufRead) -> Result<Option<McpMessage>> {
    let Some(first_line) = read_non_empty_line(reader, MAX_MCP_MESSAGE_BYTES)? else {
        return Ok(None);
    };

    let first_trimmed = trim_line_end(&first_line);
    if trim_ascii_start(first_trimmed).starts_with(b"{")
        || trim_ascii_start(first_trimmed).starts_with(b"[")
    {
        return Ok(Some(McpMessage {
            body: first_line,
            framing: StdioFraming::NewlineDelimited,
        }));
    }

    if first_line.len() > MAX_MCP_HEADER_BYTES {
        bail!("MCP headers exceed maximum size of {MAX_MCP_HEADER_BYTES} bytes");
    }
    let mut header_bytes = first_line.len();
    let mut content_length = parse_content_length_header(first_trimmed)?;
    loop {
        let remaining = MAX_MCP_HEADER_BYTES.saturating_sub(header_bytes);
        let Some(line) = read_line_limited(reader, remaining)? else {
            return Ok(None);
        };
        header_bytes = header_bytes.saturating_add(line.len());
        let trimmed = trim_line_end(&line);
        if trimmed.is_empty() {
            break;
        }
        if let Some(len) = parse_content_length_header(trimmed)? {
            content_length = Some(len);
        }
    }

    let len = content_length.context("missing Content-Length")?;
    if len > MAX_MCP_MESSAGE_BYTES {
        bail!(
            "Content-Length {len} exceeds maximum MCP message size of {MAX_MCP_MESSAGE_BYTES} bytes"
        );
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(McpMessage {
        body,
        framing: StdioFraming::ContentLength,
    }))
}

pub(super) fn read_non_empty_line(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>> {
    loop {
        let Some(line) = read_line_limited(reader, max_bytes)? else {
            return Ok(None);
        };
        if !trim_line_end(&line).is_empty() {
            return Ok(Some(line));
        }
    }
}

pub(super) fn read_line_limited(
    reader: &mut impl BufRead,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    let read = reader
        .take(max_bytes.saturating_add(1) as u64)
        .read_until(b'\n', &mut line)?;
    if read == 0 {
        return Ok(None);
    }
    if line.len() > max_bytes {
        bail!("MCP line exceeds maximum size of {max_bytes} bytes");
    }
    Ok(Some(line))
}

pub(super) fn trim_line_end(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .or_else(|| line.strip_suffix(b"\r"))
        .unwrap_or(line)
}

pub(super) fn trim_ascii_start(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(line.len());
    &line[start..]
}

pub(super) fn trim_ascii(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|idx| idx + 1)
        .unwrap_or(start);
    &line[start..end]
}

pub(super) fn parse_content_length_header(line: &[u8]) -> Result<Option<usize>> {
    let Some(colon_idx) = line.iter().position(|byte| *byte == b':') else {
        return Ok(None);
    };
    let (name, value) = line.split_at(colon_idx);
    if name.eq_ignore_ascii_case(b"content-length") {
        let value = trim_ascii(&value[1..]);
        let value = std::str::from_utf8(value).context("invalid Content-Length header")?;
        return Ok(Some(value.parse::<usize>()?));
    }
    Ok(None)
}

pub(super) fn requested_protocol_version(params: Option<&Value>) -> &str {
    params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_MCP_PROTOCOL_VERSION)
}

pub(super) fn write_response(
    writer: &mut impl Write,
    framing: StdioFraming,
    response: &Value,
) -> Result<()> {
    let body = serde_json::to_vec(response)?;
    match framing {
        StdioFraming::ContentLength => {
            write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
            writer.write_all(&body)?;
        }
        StdioFraming::NewlineDelimited => {
            writer.write_all(&body)?;
            writer.write_all(b"\n")?;
        }
    }
    writer.flush()?;
    Ok(())
}
