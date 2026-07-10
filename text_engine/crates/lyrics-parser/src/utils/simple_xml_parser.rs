#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlAttribute {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XmlElement {
    pub name: String,
    pub attributes: Vec<XmlAttribute>,
    pub children: Vec<XmlElement>,
    pub text: String,
}

impl XmlElement {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attr| attr.name == name)
            .map(|attr| attr.value.as_str())
    }

    pub fn attr_any(&self, names: &[&str]) -> Option<&str> {
        self.attributes
            .iter()
            .find(|attr| names.contains(&attr.name.as_str()))
            .map(|attr| attr.value.as_str())
    }

    pub fn has_role(&self, role: &str) -> bool {
        self.attributes
            .iter()
            .any(|attr| attr.name.ends_with(":role") && attr.value == role)
    }
}

#[derive(Default)]
pub struct SimpleXmlParser;

impl SimpleXmlParser {
    pub fn parse(&self, xml: &str) -> XmlElement {
        let mut stack = Vec::<MutableElement>::new();
        let mut i = 0usize;

        while i < xml.len() {
            if xml[i..].starts_with('<') {
                if xml[i..].starts_with("</") {
                    let Some(end_index_rel) = xml[i + 2..].find('>') else {
                        break;
                    };
                    let end_index = i + 2 + end_index_rel;
                    if stack.len() > 1 {
                        let current = stack.pop().unwrap().to_xml_element();
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(current);
                        }
                    }
                    i = end_index + 1;
                } else if xml[i..].starts_with("<!--") {
                    let end_index = xml[i + 4..]
                        .find("-->")
                        .map(|idx| i + 4 + idx + 3)
                        .unwrap_or(xml.len());
                    i = end_index;
                } else if xml[i..].starts_with("<?") {
                    let end_index = xml[i + 2..]
                        .find("?>")
                        .map(|idx| i + 2 + idx + 2)
                        .unwrap_or(xml.len());
                    i = end_index;
                } else {
                    let Some(end_index_rel) = xml[i + 1..].find('>') else {
                        break;
                    };
                    let end_index = i + 1 + end_index_rel;
                    let mut tag_part = xml[i + 1..end_index].to_string();
                    let is_self_closing = tag_part.ends_with('/');
                    if is_self_closing {
                        tag_part.pop();
                        tag_part = tag_part.trim().to_string();
                    }

                    let (tag_name, attributes) = parse_tag_and_attributes(&tag_part);
                    let new_element = MutableElement {
                        name: tag_name,
                        attributes,
                        children: Vec::new(),
                        text: String::new(),
                    };

                    if is_self_closing {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(new_element.to_xml_element());
                        } else {
                            return new_element.to_xml_element();
                        }
                    } else {
                        stack.push(new_element);
                    }
                    i = end_index + 1;
                }
            } else {
                let next_tag_index = xml[i..].find('<').map(|idx| i + idx).unwrap_or(xml.len());
                let raw_text = &xml[i..next_tag_index];
                if !raw_text.is_empty() {
                    if let Some(current) = stack.last_mut() {
                        append_text(current, raw_text);
                    }
                }
                i = next_tag_index;
            }
        }

        stack
            .first()
            .map(|e| e.clone().to_xml_element())
            .unwrap_or(XmlElement {
                name: String::new(),
                attributes: Vec::new(),
                children: Vec::new(),
                text: String::new(),
            })
    }
}

fn append_text(element: &mut MutableElement, raw_text: &str) {
    if raw_text.trim().is_empty() {
        let is_layout_whitespace = raw_text.contains('\n') || raw_text.contains('\r');
        if !is_layout_whitespace {
            element.children.push(XmlElement {
                name: "#text".to_string(),
                attributes: Vec::new(),
                children: Vec::new(),
                text: raw_text.to_string(),
            });
        }
        return;
    }
    element.text.push_str(raw_text);
}

fn parse_tag_and_attributes(tag_part: &str) -> (String, Vec<XmlAttribute>) {
    let first_space = tag_part
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, _)| index);
    let Some(first_space) = first_space else {
        return (tag_part.to_string(), Vec::new());
    };

    let tag_name = tag_part[..first_space].to_string();
    let mut attributes = Vec::new();
    let mut i = first_space + 1;
    let bytes = tag_part.as_bytes();

    while i < tag_part.len() {
        while i < tag_part.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= tag_part.len() {
            break;
        }
        let Some(equals_rel) = tag_part[i..].find('=') else {
            break;
        };
        let equals_index = i + equals_rel;
        let attr_name = tag_part[i..equals_index].trim().to_string();
        i = equals_index + 1;
        while i < tag_part.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= tag_part.len() {
            break;
        }

        let quote = bytes[i] as char;
        if quote == '"' || quote == '\'' {
            let Some(next_quote_rel) = tag_part[i + 1..].find(quote) else {
                break;
            };
            let next_quote = i + 1 + next_quote_rel;
            attributes.push(XmlAttribute {
                name: attr_name,
                value: tag_part[i + 1..next_quote].to_string(),
            });
            i = next_quote + 1;
        } else {
            let mut next_space = i;
            while next_space < tag_part.len() && !bytes[next_space].is_ascii_whitespace() {
                next_space += 1;
            }
            attributes.push(XmlAttribute {
                name: attr_name,
                value: tag_part[i..next_space].to_string(),
            });
            i = next_space;
        }
    }

    (tag_name, attributes)
}

#[derive(Clone)]
struct MutableElement {
    name: String,
    attributes: Vec<XmlAttribute>,
    children: Vec<XmlElement>,
    text: String,
}

impl MutableElement {
    fn to_xml_element(self) -> XmlElement {
        XmlElement {
            name: self.name,
            attributes: self.attributes,
            children: self.children,
            text: self.text,
        }
    }
}
