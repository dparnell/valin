use std::fmt::{Display, Write};

use smallvec::SmallVec;
use tokio::time::Instant;

use crate::use_editable::EditorData;

#[derive(Clone, Debug)]
pub enum SyntaxType {
    Number,
    String,
    Keyword,
    SpecialKeyword,
    Punctuation,
    Unknown,
    Property,
    Comment,
}

impl SyntaxType {
    pub fn color(&self) -> &str {
        match self {
            SyntaxType::Keyword => "rgb(215, 85, 67)",
            SyntaxType::String => "rgb(184, 187, 38)",
            SyntaxType::Number => "rgb(211, 134, 155)",
            SyntaxType::Punctuation => "rgb(104, 157, 96)",
            SyntaxType::Unknown => "rgb(189, 174, 147)",
            SyntaxType::Property => "rgb(168, 168, 37)",
            SyntaxType::SpecialKeyword => "rgb(211, 134, 155)",
            SyntaxType::Comment => "gray",
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SyntaxSemantic {
    Unknown,
    PropertyAccess,
}

impl From<SyntaxSemantic> for SyntaxType {
    fn from(value: SyntaxSemantic) -> Self {
        match value {
            SyntaxSemantic::PropertyAccess => SyntaxType::Property,
            SyntaxSemantic::Unknown => SyntaxType::Unknown,
        }
    }
}

#[derive(PartialEq, Clone, Debug)]
pub enum TextType {
    String(String),
    Char(char),
}

impl Display for TextType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Char(c) => f.write_char(*c),
        }
    }
}

pub type SyntaxLine = SmallVec<[(SyntaxType, TextType); 5]>;
pub type SyntaxBlocks = Vec<SyntaxLine>;

const GENERIC_KEYWORDS: &[&str] = &[
    "use", "impl", "if", "let", "fn", "struct", "enum", "const", "pub", "crate", "else", "mut",
    "for", "i8", "u8", "i16", "u16", "i32", "u32", "f32", "i64", "u64", "f64", "i128", "u128",
    "usize", "isize", "move", "async", "in", "of",
];

const SPECIAL_KEYWORDS: &[&str] = &["self", "Self", "false", "true"];

const CHARS: &[char] = &[
    '.', '{', '}', '(', ')', '=', ';', '\'', ',', '>', '<', ']', '[', '#', '&', '-', '+', '^', '\\',
];

#[derive(PartialEq, Clone, Debug)]
enum CommentTracking {
    None,
    OneLine,
    MultiLine,
}

fn push_unknown(
    unknown_stack: &mut Option<String>,
    syntax_blocks: &mut SyntaxLine,
    last_semantic: &mut SyntaxSemantic,
) {
    if let Some(word) = unknown_stack {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            return;
        }
        let word = unknown_stack.take().unwrap();
        if GENERIC_KEYWORDS.contains(&word.as_str().trim()) {
            syntax_blocks.push((SyntaxType::Keyword, TextType::String(word)));
        } else if SPECIAL_KEYWORDS.contains(&word.as_str().trim()) || word.to_uppercase() == word {
            syntax_blocks.push((SyntaxType::SpecialKeyword, TextType::String(word)));
        } else {
            syntax_blocks.push(((*last_semantic).into(), TextType::String(word)));
        }
        *last_semantic = SyntaxSemantic::Unknown;
    }
}

fn push_space(unknown_stack: &mut Option<String>, syntax_blocks: &mut SyntaxLine) {
    if let Some(word) = &unknown_stack {
        let trimmed = word.trim();
        if trimmed.is_empty() {
            syntax_blocks.push((
                SyntaxType::Unknown,
                TextType::String(unknown_stack.take().unwrap()),
            ));
        }
    }
}

pub fn parse(editor: &EditorData, syntax_blocks: &mut SyntaxBlocks) {
    syntax_blocks.clear();

    let timer = Instant::now();

    let mut tracking_comment = CommentTracking::None;
    let mut comment_stack: Option<String> = None;

    let mut tracking_string = false;
    let mut string_stack: Option<String> = None;

    let mut unknown_stack: Option<String> = None;
    let mut last_semantic = SyntaxSemantic::Unknown;

    let mut line = SyntaxLine::new();

    for (i, ch) in editor.rope().chars().enumerate() {
        let is_last_line = editor.rope().len_chars() - 1 == i;
        if ch == '\r' {
            continue;
        }
        if tracking_string && ch == '"' {
            push_unknown(&mut unknown_stack, &mut line, &mut last_semantic);
            let mut st = string_stack.take().unwrap_or_default();

            // Strings
            st.push('"');
            line.push((SyntaxType::String, TextType::String(st)));
            tracking_string = false;
        } else if tracking_comment == CommentTracking::None && ch == '"' {
            string_stack = Some(String::from('"'));
            tracking_string = true;
        } else if tracking_comment != CommentTracking::None {
            if let Some(ct) = comment_stack.as_mut() {
                ct.push(ch);

                // Check end of multine comments
                if ch == '/' && ct.ends_with("*/") {
                    line.push((
                        SyntaxType::Comment,
                        TextType::String(comment_stack.take().unwrap()),
                    ));
                    tracking_comment = CommentTracking::None;
                }
            } else {
                comment_stack = Some(String::from(ch));
            }
        } else if tracking_string {
            if let Some(st) = string_stack.as_mut() {
                st.push(ch);
            } else {
                string_stack = Some(String::from(ch));
            }
        } else if ch.is_numeric() {
            push_unknown(&mut unknown_stack, &mut line, &mut last_semantic);
            // Numbers
            line.push((SyntaxType::Number, TextType::Char(ch)));

            last_semantic = SyntaxSemantic::Unknown;
        } else if CHARS.contains(&ch) {
            push_unknown(&mut unknown_stack, &mut line, &mut last_semantic);

            if ch == '.' && last_semantic != SyntaxSemantic::PropertyAccess {
                last_semantic = SyntaxSemantic::PropertyAccess;
            }
            // Punctuation
            line.push((SyntaxType::Punctuation, TextType::Char(ch)));
        } else {
            if tracking_comment == CommentTracking::None && (ch == '*' || ch == '/') {
                if let Some(us) = unknown_stack.as_mut() {
                    if us == "/" {
                        comment_stack = unknown_stack.take();
                        if let Some(ct) = comment_stack.as_mut() {
                            ct.push(ch);
                        } else {
                            comment_stack = Some(ch.to_string());
                        }
                        if ch == '*' {
                            tracking_comment = CommentTracking::MultiLine
                        } else if ch == '/' {
                            tracking_comment = CommentTracking::OneLine
                        }
                    }
                }
            }

            if ch.is_whitespace() {
                push_unknown(&mut unknown_stack, &mut line, &mut last_semantic);
            }

            if let Some(ks) = unknown_stack.as_mut() {
                ks.push(ch);
            } else {
                unknown_stack = Some(ch.to_string())
            }

            if ch.is_whitespace() {
                push_space(&mut unknown_stack, &mut line);
            }
        }

        if ch == '\n' || is_last_line {
            if tracking_comment != CommentTracking::None {
                if let Some(ct) = comment_stack.take() {
                    line.push((SyntaxType::Comment, TextType::String(ct)));
                }
                if tracking_comment == CommentTracking::OneLine {
                    tracking_comment = CommentTracking::None
                }
            }

            push_unknown(&mut unknown_stack, &mut line, &mut last_semantic);
            push_space(&mut unknown_stack, &mut line);
            if let Some(st) = string_stack.take() {
                line.push((SyntaxType::String, TextType::String(st)));
            }
            syntax_blocks.push(line.drain(0..).collect());
        }
    }

    println!("{:?}", timer.elapsed().as_millis());
}
