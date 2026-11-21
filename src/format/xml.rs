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
