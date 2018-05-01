extern crate markdown;
extern crate csv;
extern crate serde;
use markdown::{tokenize, Block, ListItem, Span};

use std::fs::File;
use std::io::Read;
use serde::ser::{Serialize, Serializer, SerializeStruct};

#[derive(Debug)]
struct ParsedMagistrature {
    faculty: Option<String>,
    university: Option<String>,
    name: Option<String>,
    links: Vec<String>,
    comment: Option<String>,
}

impl ParsedMagistrature {
    fn new() -> Self {
        Self {
            faculty: None,
            university: None,
            name: None,
            links: Vec::new(),
            comment: None,
        }
    }
    fn push_comment_subheader(&mut self, header: &str) {
        let mut comment = self.comment.take()
            .map_or_else(|| Some(String::new()), |mut s| {
                s.push_str("\n");
                Some(s)
            })
            .unwrap();
        comment.push_str(header);
        comment.push_str("\n");
        self.comment = Some(comment);
    }
    fn push_comment_text(&mut self, text: &str) {
        let mut comment = self.comment.take()
            .or_else(|| Some(String::new()))
            .unwrap();
        comment.push_str(text);
        self.comment = Some(comment);
    }
}

impl Serialize for ParsedMagistrature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer
    {
        let mut state = serializer.serialize_struct("Magistrature", 5)?;
        state.serialize_field("ВУЗ", &self.university)?;
        state.serialize_field("Факультет", &self.faculty)?;
        state.serialize_field("Магистратура", &self.name)?;
        state.serialize_field("Комментарий", &self.comment)?;
        state.serialize_field("Ссылки", &self.links.join("\n"))?;
        state.end()
    }
}

fn spans_to_text(spans: Vec<Span>, mut links: Option<&mut Vec<String>>) -> String {
    let mut text = String::new();
    for s in spans {
        match s {
            Span::Text(s) => text.push_str(&s),
            Span::Link(s, url, _title) => {
                text.push_str(&s);
                links = if let Some(mut ls) = links {
                    ls.push(url);
                    text.push_str(&*format!("[{}]", ls.len()));
                    Some(ls)
                } else {
                    None
                };
            },
            _ => {
                eprintln!("Unhandled span: {:?}", s);
            },
        }
    }
    text
}

fn list_to_text(lis: Vec<ListItem>, mut links: Option<&mut Vec<String>>)
                -> String {
    let mut text = String::new();
    for li in lis {
        text.push_str("- ");
        match li {
            ListItem::Simple(spans) => {
                links = if let Some(mut ls) = links {
                    text.push_str(&spans_to_text(spans, Some(ls)));
                    Some(ls)
                } else {
                    text.push_str(&spans_to_text(spans, None));
                    None
                };
                text.push_str("\n");
            },
            ListItem::Paragraph(_) => {
                eprintln!("Handling block list items is not implemented");
            }
        }
    }
    text
}

fn parse_description(s: &str) -> Option<(&str, &str)> {
    if let Some(colon) = s.find(':') {
        let (start, end) = s.split_at(colon);
        Some((start, end.split_at(1).1.trim()))
    } else {
        None
    }
}

fn parse_description_list(lis: Vec<ListItem>,
                          mut links: Option<&mut Vec<String>>)
                          -> (Option<String>, Option<String>, Option<String>) {
    let mut university = None;
    let mut faculty = None;
    let mut speciality = None;
    for li in lis {
        if let ListItem::Simple(spans) = li {
            let (text, mut ls) = if let Some(mut ls) = links {
                let text = spans_to_text(spans, Some(ls));
                (text, Some(ls))
            } else {
                (spans_to_text(spans, None), None)
            };
            links = ls;
            if let Some((name, value)) = parse_description(&text) {
                match name {
                    "Факультет" => faculty = Some(value.to_owned()),
                    "Специальность" => speciality = Some(value.to_owned()),
                    "ВУЗ" => university = Some(value.to_owned()),
                    _ => eprintln!("Unknown key in description list"),
                };
            } else {
                eprintln!("Description list contains unparseable entry");
            }
        } else {
            eprintln!("I dont know what to do with block list items in description list");
        }
    }
    (university, faculty, speciality)
}


fn extract_mags(blocks: Vec<Block>) -> Vec<ParsedMagistrature> {
    let mut mag = ParsedMagistrature::new();
    let mut mags = Vec::new();
    #[derive(PartialEq)]
    enum State {
        Start, TopLevel, ExpectDescriptionList,
        MagistratureSection, CommentSection,
    };
    let mut state = State::Start;
    for b in blocks {
        eprintln!("DEBUG: block: {:?}", b);
        match b {
            Block::Header(_, 1) => {
                state = State::TopLevel;
            },
            Block::Header(_, 2) => {
                if state != State::TopLevel {
                    mags.push(mag);
                    mag = ParsedMagistrature::new();
                }
                state = State::ExpectDescriptionList;
            },
            Block::Header(spans, 3) => {
                let text = spans_to_text(spans, None);
                if text == "Обоснование" {
                    state = State::CommentSection;
                }
            },
            Block::Header(spans, level) => {
                if level > 3 {
                    if state == State::CommentSection {
                        let text = spans_to_text(spans, Some(&mut mag.links));
                        mag.push_comment_subheader(&text);
                    } else {
                        eprintln!("Unknown header found: '{:?}'", spans);
                    }
                } else {
                    eprintln!("WTF?");
                }
            },
            Block::Paragraph(spans) => {
                match state {
                    State::CommentSection => {
                        let text = spans_to_text(spans, Some(&mut mag.links));
                        mag.push_comment_text(&text);
                    },
                    _ => {
                        eprintln!("Unknown paragraph found: {:?}", spans);
                    }
                }
            },
            Block::UnorderedList(lis) => {
                match state {
                    State::ExpectDescriptionList => {
                        let (university, faculty, name) =
                            parse_description_list(lis, Some(&mut mag.links));
                        if university.is_none() {
                            eprintln!("Desription for magistrature lacks university");
                        }
                        if faculty.is_none() {
                            eprintln!("Desription for magistrature lacks faculty");
                        }
                        if name.is_none() {
                            eprintln!("Desription for magistrature lacks program name");
                        }
                        mag.university = university;
                        mag.faculty = faculty;
                        mag.name = name;
                        state = State::MagistratureSection;
                        eprintln!("DEBUG: description parsed: {:?}", mag);
                    },
                    State::CommentSection => {
                        let text = list_to_text(lis, Some(&mut mag.links));
                        mag.push_comment_text(&text);
                    },
                    _ => {
                        eprintln!("Unexpected list");
                    },
                }
            },
            _ => {
                eprintln!("Unknown block");
            },
        }
    }
    mags
}

fn main() {
    let mut file = File::open("../../magistratures/СПбГУ.md")
        .expect("Unable to open file");
    let mut text = String::new();
    file.read_to_string(&mut text)
        .expect("Unable to read file into string");
    let mut mags = extract_mags(tokenize(&*text));
    eprintln!("Mags are:\n{:?}", mags);
    let mut wtr = csv::Writer::from_writer(std::io::stdout());
    for mag in mags.drain(0..) {
        wtr.serialize(mag).expect("Unable to write CSV");
    }
    wtr.flush().expect("Unable to flush CSV");
}
