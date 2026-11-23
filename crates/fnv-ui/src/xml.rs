//! A forgiving reader for the menu files' XML (docs/spec-notes/menu-xml.md §1).
//!
//! The game's own files are not well-formed XML [observed: 43 of the 126 files under
//! `menus\` fail a strict parser]: entities are used that no file declares (`&true;`,
//! `&-sInventoryItems;`), a file may hold several top-level elements (the prefabs are
//! fragments), end tags may not match, and one attribute is missing its opening quote.
//! So this reader never fails: it builds what it can and lists what it had to mend.

/// One piece of an element's content.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    /// Text as written, with the predefined entities (`&amp;` …, `&#N;`) decoded.
    Text(String),
    /// A named entity the reader does not know, `&name;`, kept by name (without `&`
    /// and `;`). The menu files' constants (`&true;`, `&center;`) and game-setting
    /// strings (`&-sName;`) are these.
    Entity(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    /// The tag as written; compare with [`Element::is`].
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    /// The 1-based line the start tag is on.
    pub line: u32,
}

impl Element {
    /// Whether the tag is `name`, ignoring case.
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    /// An attribute's value, its name compared without case.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The element children.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        })
    }
}

/// What a file read into.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    /// The top-level nodes (a menu file has one `<menu>`, a prefab several).
    pub nodes: Vec<Node>,
    /// What the reader mended, one line each, with the line number.
    pub warnings: Vec<String>,
}

/// Bytes of a menu file as text: UTF-8 when it is, else 8-bit text in this run's code
/// page (`fnv_util::text`, #1439) [hypothesis: the files are 8-bit Windows text in the
/// install's code page; every vanilla English one is ASCII].
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => fnv_util::text::decode(bytes),
    }
}

/// A character of a tag's or an attribute's name. `+` too, which XML does not allow but the
/// game's reader evidently does [observed: Vanilla UI Plus names traits `_VUI+Fade`,
/// `_VUI+OverwriteVerify`, `_VUI+brightness` and reads them back by those names; cut at the
/// `+` they all became one `_VUI`, its self-check failed and showed its "installation
/// error" text, and its option titles lost their brightness, #2298].
fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '+')
}

/// How many elements may be open inside one another. The trees this reader builds are
/// walked by functions that call themselves once per level (and dropped the same way), so
/// a file with tens of thousands of nested start tags would overflow the stack (#3156).
/// [hypothesis: far beyond any real menu; the limit is NVForever's own, the original's
/// is not known.]
pub const MAX_DEPTH: usize = 256;

/// Reads `src`; never fails.
pub fn parse(src: &str) -> Doc {
    let chars: Vec<char> = src.chars().collect();
    let mut p = Parser { s: &chars, i: 0, line: 1, warnings: Vec::new() };
    // The open elements; the bottom one is the document.
    let mut stack: Vec<Element> = vec![Element::default()];
    // How many elements past `MAX_DEPTH` are open; they and what is in them are left out.
    let mut too_deep = 0usize;
    while p.i < chars.len() {
        if p.peek_str("<!--") {
            let line = p.line;
            p.advance(4);
            if !p.skip_past("-->") {
                p.warn(line, "comment not closed");
            }
        } else if p.peek_str("<![CDATA[") {
            p.advance(9);
            let start = p.i;
            let line = p.line;
            let closed = p.skip_past("]]>");
            let end = if closed { p.i - 3 } else { p.i };
            if !closed {
                p.warn(line, "CDATA not closed");
            }
            if too_deep == 0 {
                let text: String = chars[start..end].iter().collect();
                push_text(&mut stack, text);
            }
        } else if p.peek_str("<?") || p.peek_str("<!") {
            p.advance(2);
            p.skip_past(">");
        } else if p.peek_str("</") {
            let line = p.line;
            p.advance(2);
            let name = p.take_while(name_char);
            p.skip_past(">");
            if too_deep > 0 {
                too_deep -= 1;
            } else {
                close(&mut stack, &name, line, &mut p.warnings);
            }
        } else if p.peek() == Some('<') && p.peek_at(1).is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
            let line = p.line;
            p.advance(1);
            let name = p.take_while(name_char);
            let (attrs, empty) = p.attributes(line);
            let el = Element { name, attrs, children: Vec::new(), line };
            let missing_slash_at = if too_deep == 0
                && !empty
                && el.attrs.is_empty()
                && !is_container(&el.name)
                && !el.is("include")
                && !is_operator(&el.name)
            {
                open_trait_for_missing_slash(&stack, &el.name)
            } else {
                None
            };
            if too_deep > 0 {
                too_deep += usize::from(!empty);
            } else if let Some(at) = missing_slash_at {
                close_missing_slash(&mut stack, at, &el.name, line, &mut p.warnings);
            } else if !empty && stack.len() > MAX_DEPTH {
                p.warn(line, &format!("<{}> is nested more than {MAX_DEPTH} deep; left out with what is in it", el.name));
                too_deep = 1;
            } else if empty {
                stack.last_mut().unwrap().children.push(Node::Element(el));
            } else {
                stack.push(el);
            }
        } else {
            // Text up to the next tag; a `<` that starts no tag is text.
            let start = p.i;
            p.advance(1);
            while p.i < chars.len() {
                let c = chars[p.i];
                if c == '<' && p.peek_at(1).is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '_' | '/' | '!' | '?')) {
                    break;
                }
                p.advance(1);
            }
            if too_deep == 0 {
                let raw: String = chars[start..p.i].iter().collect();
                push_content(&mut stack, &raw);
            }
        }
    }
    while stack.len() > 1 {
        let mut el = stack.pop().unwrap();
        p.warnings.push(format!("line {}: <{}> not closed at the end of the file", el.line, el.name));
        let hoisted = if !is_container(&el.name) && !el.is("include") {
            take_trailing_non_operators(&mut el)
        } else {
            Vec::new()
        };
        let parent = stack.last_mut().unwrap();
        parent.children.push(Node::Element(el));
        parent.children.extend(hoisted);
    }
    Doc { nodes: stack.pop().unwrap().children, warnings: p.warnings }
}

fn is_container(name: &str) -> bool {
    crate::tree::TILE_KINDS.iter().any(|k| name.eq_ignore_ascii_case(k)) || name.eq_ignore_ascii_case("template")
}

fn is_operator(name: &str) -> bool {
    crate::expr::OpKind::from_tag(name).is_some()
}

fn has_non_blank_nodes(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Text(t) => !t.trim().is_empty(),
        Node::Entity(_) | Node::Element(_) => true,
    })
}

/// Finds an open trait on `stack` with the same name as a repeated attributeless start
/// tag `<T>` when only operators (if any) are open inside it and the trait already holds
/// non-blank content [hypothesis: menu XML prefabs sometimes omit `/` on a closing trait
/// tag (`<height><copy .../><height>`), and closing the open trait keeps subsequent
/// traits and child tiles on the enclosing tile; menu-xml.md §1.1].
fn open_trait_for_missing_slash(stack: &[Element], name: &str) -> Option<usize> {
    for k in (1..stack.len()).rev() {
        let open = &stack[k];
        if open.name.eq_ignore_ascii_case(name) {
            let parent_is_container = k == 1 || is_container(&stack[k - 1].name);
            return (parent_is_container && has_non_blank_nodes(&open.children)).then_some(k);
        }
        if !is_operator(&open.name) {
            return None;
        }
    }
    None
}

fn close_missing_slash(stack: &mut Vec<Element>, at: usize, name: &str, line: u32, warnings: &mut Vec<String>) {
    while stack.len() > at {
        let el = stack.pop().unwrap();
        if stack.len() == at {
            warnings.push(format!("line {line}: <{name}> closes open <{}> (line {}); missing '/'", el.name, el.line));
        } else {
            warnings.push(format!("line {line}: <{}> (line {}) closed by <{name}>", el.name, el.line));
        }
        stack.last_mut().unwrap().children.push(Node::Element(el));
    }
}

/// Detaches contiguous trailing non-operator elements from an implicitly closed trait or
/// operator element when non-blank content precedes them [hypothesis: when an author omits
/// `</trait>` before a following sibling trait, tile, include or template inside a tile,
/// hoisting those trailing elements preserves them on the enclosing tile; menu-xml.md §1.1].
fn take_trailing_non_operators(el: &mut Element) -> Vec<Node> {
    let mut cut = el.children.len();
    let mut saw_element = false;
    for (idx, node) in el.children.iter().enumerate().rev() {
        match node {
            Node::Text(t) if t.trim().is_empty() => {}
            Node::Element(child) if !is_operator(&child.name) => {
                cut = idx;
                saw_element = true;
            }
            _ => break,
        }
    }
    if !saw_element || !has_non_blank_nodes(&el.children[..cut]) {
        return Vec::new();
    }
    el.children.split_off(cut).into_iter().filter(|n| matches!(n, Node::Element(_))).collect()
}

/// An end tag closes the innermost open element of the name it gives (and those opened
/// inside it); when none of that name is open, it closes the innermost open element
/// whatever its name; with nothing open it is dropped [hypothesis, from what works in
/// the original: MCM runs with both `<_KeyInut>` … `</_KeyInput>` and a `<_target>` whose
/// `<onlyif>` is never closed (ModListRect.xml); the first needs the second rule, the
/// second the first. Observer check: menu-xml.md §8].
fn close(stack: &mut Vec<Element>, name: &str, line: u32, warnings: &mut Vec<String>) {
    if stack.len() == 1 {
        warnings.push(format!("line {line}: </{name}> closes nothing open; dropped"));
        return;
    }
    let at = (1..stack.len()).rev().find(|&k| stack[k].name.eq_ignore_ascii_case(name)).unwrap_or(stack.len() - 1);
    let closing_enclosing_container = at < stack.len() - 1 && is_container(&stack[at].name);
    while stack.len() > at {
        let mut el = stack.pop().unwrap();
        let implicit = stack.len() > at;
        if !el.name.eq_ignore_ascii_case(name) {
            warnings.push(format!("line {line}: <{}> (line {}) closed by </{name}>", el.name, el.line));
        }
        let hoisted = if implicit && closing_enclosing_container && !is_container(&el.name) && !el.is("include") {
            take_trailing_non_operators(&mut el)
        } else {
            Vec::new()
        };
        let parent = stack.last_mut().unwrap();
        parent.children.push(Node::Element(el));
        parent.children.extend(hoisted);
    }
}

fn push_text(stack: &mut [Element], text: String) {
    let top = stack.last_mut().unwrap();
    if let Some(Node::Text(t)) = top.children.last_mut() {
        t.push_str(&text);
    } else {
        top.children.push(Node::Text(text));
    }
}

/// Text with entities: the predefined ones decoded into the text, the others kept as
/// [`Node::Entity`].
fn push_content(stack: &mut [Element], raw: &str) {
    let mut text = String::new();
    let mut rest = raw;
    while let Some(k) = rest.find('&') {
        text.push_str(&rest[..k]);
        let after = &rest[k + 1..];
        let len = after.find(|c: char| !(name_char(c) || c == '#')).unwrap_or(after.len());
        if len > 0 && after[len..].starts_with(';') {
            let name = &after[..len];
            match predefined(name) {
                Some(c) => text.push(c),
                None => {
                    if !text.is_empty() {
                        push_text(stack, std::mem::take(&mut text));
                    }
                    stack.last_mut().unwrap().children.push(Node::Entity(name.to_string()));
                }
            }
            rest = &after[len + 1..];
        } else {
            text.push('&');
            rest = after;
        }
    }
    text.push_str(rest);
    if !text.is_empty() {
        push_text(stack, text);
    }
}

fn predefined(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let n = name.strip_prefix('#')?;
            let v = match n.strip_prefix(['x', 'X']) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(v)?
        }
    })
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
    line: u32,
    warnings: Vec<String>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.s.get(self.i + k).copied()
    }

    fn peek_str(&self, t: &str) -> bool {
        t.chars().enumerate().all(|(k, c)| self.peek_at(k) == Some(c))
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            if self.peek() == Some('\n') {
                self.line += 1;
            }
            if self.i < self.s.len() {
                self.i += 1;
            }
        }
    }

    /// Moves past the next `t`; false (at the end) when there is none.
    fn skip_past(&mut self, t: &str) -> bool {
        while self.i < self.s.len() {
            if self.peek_str(t) {
                self.advance(t.chars().count());
                return true;
            }
            self.advance(1);
        }
        false
    }

    fn take_while(&mut self, f: impl Fn(char) -> bool) -> String {
        let start = self.i;
        while self.peek().is_some_and(&f) {
            self.advance(1);
        }
        self.s[start..self.i].iter().collect()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.advance(1);
        }
    }

    fn warn(&mut self, line: u32, what: &str) {
        self.warnings.push(format!("line {line}: {what}"));
    }

    /// The attributes up to the tag's `>`; true when it ends `/>`.
    fn attributes(&mut self, line: u32) -> (Vec<(String, String)>, bool) {
        let mut attrs = Vec::new();
        loop {
            self.skip_space();
            match self.peek() {
                None => {
                    self.warn(line, "tag not closed at the end of the file");
                    return (attrs, true);
                }
                Some('>') => {
                    self.advance(1);
                    return (attrs, false);
                }
                Some('/') if self.peek_at(1) == Some('>') => {
                    self.advance(2);
                    return (attrs, true);
                }
                // A start tag cut short by the next tag (`<systemcolor</systemcolor>` in
                // two vanilla files [observed]): ends there, open, so that the end tag
                // after it closes it.
                Some('<') => {
                    self.warn(line, "tag cut short by the next one");
                    return (attrs, false);
                }
                Some(c) if name_char(c) => {
                    let name = self.take_while(name_char);
                    self.skip_space();
                    let value = if self.peek() == Some('=') {
                        self.advance(1);
                        self.skip_space();
                        self.attr_value(line, &name)
                    } else {
                        String::new()
                    };
                    attrs.push((name, value));
                }
                Some(_) => self.advance(1),
            }
        }
    }

    fn attr_value(&mut self, line: u32, name: &str) -> String {
        match self.peek() {
            Some(q @ ('"' | '\'')) => {
                self.advance(1);
                let v = self.take_while(|c| c != q);
                self.advance(1);
                v
            }
            _ => {
                // Unquoted (or missing its opening quote): up to a space or the tag's end,
                // a stray closing quote dropped.
                let v = self.take_while(|c| !c.is_whitespace() && c != '>' && c != '/');
                self.warn(line, &format!("attribute {name} not quoted"));
                v.trim_matches(['"', '\'']).to_string()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(n: &Node) -> &Element {
        match n {
            Node::Element(e) => e,
            other => panic!("not an element: {other:?}"),
        }
    }

    #[test]
    fn fragments_entities_and_comments() {
        let d = parse("<!-- head -->\n<a> 1 </a>\n<b x=\"p()\">&true; and &amp; &-sName;</b>");
        assert!(d.warnings.is_empty(), "{:?}", d.warnings);
        let els: Vec<_> = d.nodes.iter().filter(|n| matches!(n, Node::Element(_))).map(el).collect();
        assert_eq!(els.len(), 2);
        assert_eq!(els[0].children, vec![Node::Text(" 1 ".into())]);
        assert_eq!(els[1].attr("X"), Some("p()"));
        assert_eq!(
            els[1].children,
            vec![Node::Entity("true".into()), Node::Text(" and & ".into()), Node::Entity("-sName".into())]
        );
        assert_eq!(els[1].line, 3);
    }

    #[test]
    fn mends_stray_and_mismatched_end_tags() {
        // `</Bx>` (no such open) closes `<b>`; `</a>` closes the unclosed `<onlyif>` and
        // the `<a>` it is in; `</x>` closes nothing.
        let d = parse("<rect><b>1</Bx><a><onlyif>2</a><c/></rect></x><text>");
        let r = el(&d.nodes[0]);
        assert!(r.is("RECT"));
        let names: Vec<_> = r.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["b", "a", "c"]);
        assert!(el(&el(&r.children[1]).children[0]).is("onlyif"));
        assert_eq!(d.warnings.len(), 4, "{:?}", d.warnings);
        assert!(el(&d.nodes[1]).is("text"));
    }

    #[test]
    fn attribute_missing_its_opening_quote() {
        let d = parse("<rect name=item_parent\"><copy src='me()' trait=\"x\"/></rect>");
        let r = el(&d.nodes[0]);
        assert_eq!(r.attr("name"), Some("item_parent"));
        let c = el(&r.children[0]);
        assert_eq!(c.attr("src"), Some("me()"));
        assert_eq!(d.warnings.len(), 1);
    }

    #[test]
    fn a_start_tag_cut_short() {
        let d = parse("<image><systemcolor</systemcolor><locus>1</locus></image>");
        let i = el(&d.nodes[0]);
        let names: Vec<_> = i.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["systemcolor", "locus"]);
        assert_eq!(d.warnings.len(), 1, "{:?}", d.warnings);
    }

    #[test]
    fn nesting_stops_at_the_cap() {
        // 100,000 nested start tags, none closed: the tree is built, walked and dropped
        // without running out of stack (#3156).
        let deep = "<a>".repeat(100_000);
        let d = parse(&deep);
        let mut depth = 0;
        let mut at = el(&d.nodes[0]);
        while let Some(inner) = at.elements().next() {
            depth += 1;
            at = inner;
        }
        assert_eq!(depth + 1, MAX_DEPTH);
        assert!(d.warnings.iter().any(|w| w.contains("nested more than 256 deep")), "{:?}", &d.warnings[..2]);
        assert_eq!(d.clone().nodes, d.nodes);
        // What is past the cap is left out whole, and the file reads on after it.
        let src = format!("{}<deep><x/>text</deep><kept/>{}<after/>", "<a>".repeat(MAX_DEPTH), "</a>".repeat(MAX_DEPTH));
        let d = parse(&src);
        assert_eq!(d.warnings.len(), 1, "{:?}", d.warnings);
        assert_eq!(d.nodes.len(), 2);
        assert!(el(&d.nodes[1]).is("after"));
        let mut at = el(&d.nodes[0]);
        for _ in 1..MAX_DEPTH {
            at = at.elements().next().unwrap();
        }
        assert_eq!(at.children.len(), 1);
        assert!(at.elements().next().unwrap().is("kept"));
    }

    #[test]
    fn a_lone_less_than_is_text() {
        let d = parse("<a> 1 < 2 &unknown </a>");
        assert_eq!(el(&d.nodes[0]).children, vec![Node::Text(" 1 < 2 &unknown ".into())]);
    }

    #[test]
    fn latin1_fallback() {
        assert_eq!(decode(&[b'a', 0xE9]), "a\u{e9}");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'x']), "x");
    }

    #[test]
    fn missing_slash_on_repeated_trait_closes_and_preserves_siblings() {
        // Synthetic control: repeated attributeless trait start tags `<_offset> 20 <_offset>`
        // and `<height><copy .../><height>` close the open trait instead of swallowing
        // subsequent traits and child tiles.
        let src = r#"<rect name="Panel">
            <_offset> 20 <_offset>
            <height><copy src="screen()" trait="height"/><height>
            <x> 12 </x>
            <text name="Label"><string>ok</string></text>
        </rect>"#;
        let d = parse(src);
        let r = el(&d.nodes[0]);
        let names: Vec<_> = r.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["_offset", "height", "x", "text"]);
        assert_eq!(el(&r.children[1]).children, vec![Node::Text(" 20 ".into())]);
        assert!(d.warnings.iter().any(|w| w.contains("<_offset> closes open <_offset>") && w.contains("missing '/'")), "{:?}", d.warnings);
        assert!(d.warnings.iter().any(|w| w.contains("<height> closes open <height>") && w.contains("missing '/'")), "{:?}", d.warnings);
    }

    #[test]
    fn unclosed_trait_hoists_trailing_sibling_traits_and_tiles_on_container_close() {
        // Synthetic control: omitted `</alpha>` before `<brightness>` and a child tile
        // hoists those non-operator siblings onto the enclosing container without
        // disturbing explicitly closed traits that contain unknown operator tags.
        let src = r#"<image name="Gauge">
            <alpha>
                <copy> 0 </copy>
                <add> 128 </add>
                <brightness> 200 </brightness>
                <rect name="Child"><x> 4 </x></rect>
        </image>
        <rect name="Explicit">
            <_flag><copy> 1 </copy><unknown_op> 2 </unknown_op></_flag>
        </rect>"#;
        let d = parse(src);
        let els: Vec<_> = d.nodes.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            _ => None,
        }).collect();
        let img = els[0];
        let img_names: Vec<_> = img.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(img_names, ["alpha", "brightness", "rect"]);
        let alpha_ops: Vec<_> = img.elements().next().unwrap().elements().map(|e| e.name.as_str()).collect();
        assert_eq!(alpha_ops, ["copy", "add"]);
        // Explicitly closed trait retains unknown inner tag for ops_of to warn on as before.
        let explicit_flag = els[1].elements().next().unwrap();
        let flag_kids: Vec<_> = explicit_flag.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(flag_kids, ["copy", "unknown_op"]);
    }

    #[test]
    fn unclosed_chained_traits_and_eof_fragment_hoist_without_affecting_nested_operators() {
        let chained = r#"<rect name="Box"><alpha> 120 <brightness> 240 <width> 64 </width></rect>"#;
        let d = parse(chained);
        let r = el(&d.nodes[0]);
        let names: Vec<_> = r.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["alpha", "brightness", "width"]);
        // Top-level prefab fragment closed at EOF.
        let eof = r#"<alpha> 90 <brightness> 180 </brightness>"#;
        let d2 = parse(eof);
        let top_names: Vec<_> = d2.nodes.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e.name.as_str()),
            _ => None,
        }).collect();
        assert_eq!(top_names, ["alpha", "brightness"]);
        // Nested operators of the same name are untouched by missing-slash closing.
        let nested_op = r#"<rect name="Calc"><x><add> 1 <add> 2 </add></add></x></rect>"#;
        let d3 = parse(nested_op);
        assert!(d3.warnings.is_empty(), "{:?}", d3.warnings);
    }
}
