use crate::error::{Result, invalid};
use crate::filters;
use crate::object::PdfObject;
use crate::parse::{PageNode, ParsedDocument, Parser, decode_text_string, is_delimiter, is_whitespace, resolve_object};

pub(crate) struct Extraction {
    pub markdown: String,
    pub losses: Vec<Loss>,
}

pub(crate) struct Loss {
    pub code: String,
    pub page: usize,
    pub message: String,
}

/// Extracts visible text from content streams decoded through their filters.
///
/// `/FlateDecode` is decoded in full. Every other filter is reported as a loss
/// and keeps its raw bytes, which is why the capability matrix still calls text
/// extraction partial rather than supported.
pub(crate) fn extract(document: &ParsedDocument) -> Result<Extraction> {
    let mut pages = Vec::new();
    let mut losses = Vec::new();
    for (index, page) in document.pages.iter().enumerate() {
        let number = index.saturating_add(1);
        let streams = contents_of(document, page)?;
        let mut lines = Vec::new();
        for stream in streams {
            match filters::decode(stream, &document.objects) {
                Ok(bytes) => extract_stream(&bytes, &mut lines),
                Err(error) if error.code() == "format/unsupported" => {
                    losses.push(Loss {
                        code: "content/filter-unsupported".to_string(),
                        page: number,
                        message: error.message().to_string(),
                    });
                }
                Err(error) => {
                    losses.push(Loss {
                        code: "content/filter-invalid".to_string(),
                        page: number,
                        message: error.message().to_string(),
                    });
                }
            }
        }
        let text = lines.join("\n");
        if !text.is_empty() {
            pages.push(text);
        }
    }
    Ok(Extraction { markdown: pages.join("\n\n"), losses })
}

fn contents_of<'a>(document: &'a ParsedDocument, page: &'a PageNode) -> Result<Vec<&'a crate::object::PdfStream>> {
    let Some(value) = page.own.get("Contents") else {
        return Ok(Vec::new());
    };
    let mut streams = Vec::new();
    match resolve_object(&document.objects, value)? {
        PdfObject::Null => {}
        PdfObject::Stream(stream) => streams.push(stream),
        PdfObject::Array(items) => {
            for item in items {
                match resolve_object(&document.objects, item)? {
                    PdfObject::Stream(stream) => streams.push(stream),
                    PdfObject::Null => {}
                    _ => return Err(invalid("/Contents array holds a non-stream object")),
                }
            }
        }
        _ => return Err(invalid("/Contents is not a stream or an array of streams")),
    }
    Ok(streams)
}

enum Token {
    Object(PdfObject),
    Operator(String),
}

fn next_token(parser: &mut Parser<'_>) -> Option<Result<Token>> {
    parser.skip_ws();
    let byte = parser.peek()?;
    let object_start = byte == b'/'
        || byte == b'('
        || byte == b'<'
        || byte == b'['
        || byte == b'+'
        || byte == b'-'
        || byte == b'.'
        || byte.is_ascii_digit();
    if object_start {
        return Some(parser.parse_object(0).map(Token::Object));
    }

    let start = parser.pos;
    while let Some(current) = parser.peek() {
        if is_whitespace(current) || is_delimiter(current) {
            break;
        }
        parser.bump();
    }
    if parser.pos == start {
        parser.bump();
        return next_token(parser);
    }
    let token = parser.data.get(start..parser.pos)?;
    Some(Ok(Token::Operator(String::from_utf8_lossy(token).into_owned())))
}

fn extract_stream(bytes: &[u8], lines: &mut Vec<String>) {
    let mut parser = Parser::new(bytes);
    let mut operands: Vec<PdfObject> = Vec::new();
    let mut line = String::new();

    while let Some(token) = next_token(&mut parser) {
        let Ok(token) = token else {
            break;
        };
        match token {
            Token::Object(object) => operands.push(object),
            Token::Operator(operator) => {
                match operator.as_str() {
                    "Tj" => {
                        if let Some(PdfObject::String(value)) = operands.last() {
                            line.push_str(&decode_text_string(value));
                        }
                    }
                    "TJ" => {
                        if let Some(PdfObject::Array(items)) = operands.last() {
                            for item in items {
                                match item {
                                    PdfObject::String(value) => line.push_str(&decode_text_string(value)),
                                    PdfObject::Integer(offset) if *offset <= -100 => push_word_gap(&mut line),
                                    PdfObject::Real(offset) if *offset <= -100.0 => push_word_gap(&mut line),
                                    _ => {}
                                }
                            }
                        }
                    }
                    "'" => {
                        if let Some(PdfObject::String(value)) = operands.last() {
                            line.push_str(&decode_text_string(value));
                        }
                        push_line(lines, &mut line);
                    }
                    "\"" => {
                        if let Some(PdfObject::String(value)) = operands.last() {
                            line.push_str(&decode_text_string(value));
                        }
                        push_line(lines, &mut line);
                    }
                    "Td" | "TD" | "T*" | "Tm" => push_line(lines, &mut line),
                    "ET" => push_line(lines, &mut line),
                    "BI" => skip_inline_image(&mut parser),
                    _ => {}
                }
                operands.clear();
            }
        }
    }
    push_line(lines, &mut line);
}

fn push_word_gap(line: &mut String) {
    if !line.is_empty() && !line.ends_with(' ') {
        line.push(' ');
    }
}

fn push_line(lines: &mut Vec<String>, line: &mut String) {
    let text = line.trim_end().to_string();
    if !text.is_empty() {
        lines.push(text);
    }
    line.clear();
}

fn skip_inline_image(parser: &mut Parser<'_>) {
    while let Some(byte) = parser.peek() {
        if byte == b'E'
            && parser.data.get(parser.pos + 1) == Some(&b'I')
            && parser
                .data
                .get(parser.pos.wrapping_sub(1))
                .copied()
                .map(|previous| is_whitespace(previous) || is_delimiter(previous))
                .unwrap_or(true)
        {
            parser.pos = parser.pos.saturating_add(2).min(parser.data.len());
            return;
        }
        parser.bump();
    }
}
