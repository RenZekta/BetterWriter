//! Minimal XML writer shared by the Guitar Pro and MuseScore exporters.

/// A small XML builder: indentation is cosmetic, escaping is not.
pub(super) struct Xml {
    pub(super) out: String,
    pub(super) depth: usize,
}

impl Xml {
    pub(super) fn new() -> Self {
        Self {
            out: String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n"),
            depth: 0,
        }
    }

    pub(super) fn indent(&mut self) {
        for _ in 0..self.depth {
            self.out.push(' ');
        }
    }

    pub(super) fn open(&mut self, tag: &str) {
        self.open_with(tag, &[]);
    }

    pub(super) fn open_with(&mut self, tag: &str, attributes: &[(&str, String)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.attributes(attributes);
        self.out.push_str(">\n");
        self.depth += 1;
    }

    pub(super) fn close(&mut self, tag: &str) {
        self.depth -= 1;
        self.indent();
        self.out.push_str("</");
        self.out.push_str(tag);
        self.out.push_str(">\n");
    }

    pub(super) fn attributes(&mut self, attributes: &[(&str, String)]) {
        for (name, value) in attributes {
            self.out.push_str(&format!(" {name}=\"{}\"", escape(value)));
        }
    }

    pub(super) fn empty(&mut self, tag: &str) {
        self.empty_with(tag, &[]);
    }

    pub(super) fn empty_with(&mut self, tag: &str, attributes: &[(&str, String)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.attributes(attributes);
        self.out.push_str("/>\n");
    }

    pub(super) fn text_with(&mut self, tag: &str, attributes: &[(&str, String)], value: impl std::fmt::Display) {
        self.indent();
        self.out.push('<');
        self.out.push_str(tag);
        self.attributes(attributes);
        self.out
            .push_str(&format!(">{}</{tag}>\n", escape(&value.to_string())));
    }

    pub(super) fn text(&mut self, tag: &str, value: impl std::fmt::Display) {
        self.indent();
        self.out
            .push_str(&format!("<{tag}>{}</{tag}>\n", escape(&value.to_string())));
    }

    pub(super) fn cdata(&mut self, tag: &str, value: &str) {
        self.indent();
        let body = value.replace("]]>", "]]]]><![CDATA[>");
        self.out.push_str(&format!("<{tag}><![CDATA[{body}]]></{tag}>\n"));
    }

    /// `<Property name="..."><kind>value</kind></Property>`; no value
    /// writes the bare `<kind/>` flag.
    pub(super) fn property(&mut self, name: &str, kind: &str, value: Option<String>) {
        self.indent();
        self.out.push_str(&format!("<Property name=\"{}\">", escape(name)));
        match value {
            Some(value) => self.out.push_str(&format!("<{kind}>{}</{kind}>", escape(&value))),
            None => self.out.push_str(&format!("<{kind}/>")),
        }
        self.out.push_str("</Property>\n");
    }
}

pub(super) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// An element of a parsed document: the readers of GPIF and MuseScore files
/// walk these. Text is what lies directly in the element, entities decoded.
#[derive(Debug, Default, Clone)]
pub(super) struct Node {
    pub(super) name: String,
    pub(super) attributes: Vec<(String, String)>,
    pub(super) children: Vec<Node>,
    pub(super) text: String,
}

impl Node {
    pub(super) fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub(super) fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }

    pub(super) fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// The trimmed text of the first child called `name`.
    pub(super) fn text_of(&self, name: &str) -> Option<&str> {
        self.child(name).map(|child| child.text.trim())
    }

    pub(super) fn number_of<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
        self.text_of(name)?.parse().ok()
    }
}

fn unescape(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|dec| dec.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn malformed(what: &str) -> super::FormatError {
    super::FormatError::UnsupportedFormat(format!("malformed XML: {what}"))
}

/// Parses a document into its root element. Comments, processing
/// instructions and the doctype are skipped; CDATA becomes text.
pub(super) fn parse(source: &str) -> Result<Node, super::FormatError> {
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;
    let mut rest = source.trim_start_matches('\u{feff}');
    while !rest.is_empty() {
        let Some(open) = rest.find('<') else {
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&unescape(rest));
            }
            break;
        };
        if open > 0
            && let Some(top) = stack.last_mut()
        {
            top.text.push_str(&unescape(&rest[..open]));
        }
        rest = &rest[open..];
        if let Some(body) = rest.strip_prefix("<![CDATA[") {
            let end = body.find("]]>").ok_or_else(|| malformed("unterminated CDATA"))?;
            if let Some(top) = stack.last_mut() {
                top.text.push_str(&body[..end]);
            }
            rest = &body[end + 3..];
        } else if let Some(body) = rest.strip_prefix("<!--") {
            let end = body.find("-->").ok_or_else(|| malformed("unterminated comment"))?;
            rest = &body[end + 3..];
        } else if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest.find('>').ok_or_else(|| malformed("unterminated declaration"))?;
            rest = &rest[end + 1..];
        } else {
            let end = tag_end(rest).ok_or_else(|| malformed("unterminated tag"))?;
            let tag = &rest[1..end];
            rest = &rest[end + 1..];
            if let Some(name) = tag.strip_prefix('/') {
                let node = stack.pop().ok_or_else(|| malformed("unexpected closing tag"))?;
                if node.name != name.trim() {
                    return Err(malformed(&format!("</{}> closes <{}>", name.trim(), node.name)));
                }
                match stack.last_mut() {
                    Some(parent) => parent.children.push(node),
                    None => root = Some(node),
                }
            } else {
                let empty = tag.ends_with('/');
                let node = open_tag(tag.trim_end_matches('/'))?;
                if empty {
                    match stack.last_mut() {
                        Some(parent) => parent.children.push(node),
                        None => root = Some(node),
                    }
                } else {
                    stack.push(node);
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err(malformed("unclosed element"));
    }
    root.ok_or_else(|| malformed("no root element"))
}

/// Index of the `>` that ends the tag starting at `text[0]`, skipping any
/// inside a quoted attribute value.
fn tag_end(text: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (at, ch) in text.char_indices() {
        match (quote, ch) {
            (Some(open), ch) if ch == open => quote = None,
            (None, '"' | '\'') => quote = Some(ch),
            (None, '>') => return Some(at),
            _ => {}
        }
    }
    None
}

fn open_tag(tag: &str) -> Result<Node, super::FormatError> {
    let tag = tag.trim();
    let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
    let mut node = Node {
        name: tag[..name_end].to_owned(),
        ..Node::default()
    };
    let mut rest = tag[name_end..].trim_start();
    while !rest.is_empty() {
        let equals = rest.find('=').ok_or_else(|| malformed("attribute without a value"))?;
        let key = rest[..equals].trim().to_owned();
        let after = rest[equals + 1..].trim_start();
        let quote = after
            .chars()
            .next()
            .filter(|ch| matches!(ch, '"' | '\''))
            .ok_or_else(|| malformed("unquoted attribute"))?;
        let value = &after[1..];
        let end = value.find(quote).ok_or_else(|| malformed("unterminated attribute"))?;
        node.attributes.push((key, unescape(&value[..end])));
        rest = value[end + 1..].trim_start();
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_is_parsed_into_elements_attributes_and_text() {
        let root = parse(
            "<?xml version=\"1.0\"?>\n<!-- c --><a x=\"1&amp;2\" y='>'><b>t &lt; u</b><c/><d><![CDATA[<raw>]]></d></a>",
        )
        .unwrap();
        assert_eq!(root.name, "a");
        assert_eq!(root.attribute("x"), Some("1&2"));
        assert_eq!(root.attribute("y"), Some(">"));
        assert_eq!(root.text_of("b"), Some("t < u"));
        assert!(root.child("c").is_some());
        assert_eq!(root.text_of("d"), Some("<raw>"));
    }

    #[test]
    fn what_the_writer_escapes_the_parser_restores() {
        let mut xml = Xml::new();
        xml.open("r");
        xml.text("t", "a < b & \"c\"");
        xml.cdata("u", "x ]]> y");
        xml.close("r");
        let root = parse(&xml.out).unwrap();
        assert_eq!(root.text_of("t"), Some("a < b & \"c\""));
        assert_eq!(root.text_of("u"), Some("x ]]> y"));
    }

    #[test]
    fn mismatched_or_unclosed_markup_is_refused() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("").is_err());
    }
}
