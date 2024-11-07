// Copyright 2019 Will Page <compenguy@gmail.com>
// See LICENSE.rtf2text

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use anyhow::{Context, Result};
#[allow(unused_imports)]
use tracing::{debug, error, info, instrument, warn};

use rtf_grimoire::tokenizer::parse_finished as parse_tokens;
use rtf_grimoire::tokenizer::Token;

use crate::rtf_control;

#[derive(Clone)]
pub enum Destination {
    Text(String),
    Bytes(Vec<u8>),
}

impl Destination {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Destination::Text(text) => text.as_bytes(),
            Destination::Bytes(bytes) => bytes,
        }
    }

    fn append_text(&mut self, new_text: &str) {
        if let Destination::Text(string) = self {
            string.push_str(new_text);
        } else {
            panic!("Programmer error: attempting to add text to a byte destination");
        }
    }

    fn append_bytes(&mut self, new_bytes: &[u8]) {
        if let Destination::Bytes(bytes) = self {
            bytes.extend(new_bytes);
        } else {
            panic!("Programmer error: attempting to add bytes to a text destination");
        }
    }
}

/* TODO: It would be better to make 'values' CoW objects to reduce the number of copies we
 * make/keep
 */
#[derive(Clone)]
pub struct GroupState {
    destinations: Rc<RefCell<HashMap<String, Destination>>>,
    cur_destination: Option<String>,
    dest_encoding: Option<&'static encoding_rs::Encoding>,
    values: HashMap<String, Option<i32>>,
    opt_ignore_next_control: bool,
}

impl GroupState {
    pub fn new(destinations: Rc<RefCell<HashMap<String, Destination>>>) -> Self {
        Self {
            destinations,
            cur_destination: None,
            dest_encoding: None,
            values: HashMap::new(),
            opt_ignore_next_control: false,
        }
    }

    pub fn set_codepage(&mut self, cp: u16) {
        self.dest_encoding = codepage::to_encoding(cp);
    }

    pub fn get_encoding(&mut self) -> Option<&'static encoding_rs::Encoding> {
        self.dest_encoding
    }

    pub fn set_encoding(&mut self, encoding: Option<&'static encoding_rs::Encoding>) {
        self.dest_encoding = encoding;
    }

    pub fn set_destination(&mut self, name: &str, uses_encoding: bool) {
        self.cur_destination = Some(name.to_owned());
        let mut dest = (*self.destinations).borrow_mut();
        match dest.get(name) {
            Some(Destination::Text(string)) => {
                debug!(
                    "Switching to destination {}, with current length {})",
                    name,
                    string.len()
                );
                assert!(uses_encoding);
            }
            Some(Destination::Bytes(bytes)) => {
                debug!(
                    "Switching to destination {}, with current length {})",
                    name,
                    bytes.len()
                );
                assert!(!uses_encoding);
            }
            None => {
                if uses_encoding {
                    dest.insert(
                        name.to_string(),
                        Destination::Text(String::with_capacity(256)),
                    );
                } else {
                    dest.insert(name.to_string(), Destination::Bytes(Vec::new()));
                }
            }
        }
    }

    pub fn get_destination_name(&self) -> Option<String> {
        self.cur_destination.clone()
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let dest_name = match self.get_destination_name() {
            Some(name) => name,
            None => {
                warn!(
                    "Document format error: Document text found outside of any document group: '{:?}'",
                    bytes
                );
                return;
            }
        };
        if let Some(dest) = (*self.destinations).borrow_mut().get_mut(&dest_name) {
            match dest {
                Destination::Text(_) => {
                    if let Some(ref mut decoder) = self.dest_encoding {
                        dest.append_text(&decoder.decode(bytes).0);
                    } else {
                        warn!(
                            "Writing to a text destination ({}) with no encoding set!",
                            dest_name
                        );
                    }
                }
                Destination::Bytes(_) => dest.append_bytes(bytes),
            }
        } else {
            panic!("Programming error: specified destination {} doesn't exist after verifying its existence", dest_name);
        }
    }

    pub fn set_opt_ignore_next_control(&mut self) {
        self.opt_ignore_next_control = true;
    }

    pub fn get_and_clear_ignore_next_control(&mut self) -> bool {
        let old = self.opt_ignore_next_control;
        self.opt_ignore_next_control = false;
        old
    }

    pub fn set_value(&mut self, name: &str, value: Option<i32>) {
        self.values.insert(name.to_string(), value);
    }
}

#[derive(Clone)]
struct DocumentState {
    destinations: Rc<RefCell<HashMap<String, Destination>>>,
    group_stack: Vec<GroupState>,
}

impl DocumentState {
    fn new() -> Self {
        Self {
            destinations: Rc::new(RefCell::new(HashMap::new())),
            group_stack: Vec::new(),
        }
    }

    fn do_control_bin(&mut self, _data: &[u8], _word_is_optional: bool) {
        // We don't support handling control bins
    }

    fn do_control_symbol(&mut self, symbol: char, word_is_optional: bool) {
        let mut sym_bytes = [0; 4];
        let sym_str = symbol.encode_utf8(&mut sym_bytes);
        if let Some(group_state) = self.get_last_group_mut() {
            if let Some(symbol_handler) = rtf_control::SYMBOLS.get(sym_str) {
                symbol_handler(group_state, sym_str, None);
            } else if word_is_optional {
                info!("Skipping optional unsupported control word \\{}", symbol);
            } else {
                warn!(
                    "Unsupported/illegal control symbol \\{} (writing to document anyway)",
                    symbol
                );
                self.write_to_current_destination(format!("{symbol}").as_bytes());
            }
        } else {
            warn!(
                "Document format error: Control symbol found outside of any document group: '\\{}'",
                symbol
            );
        }
    }

    fn do_control_word(&mut self, name: &str, arg: Option<i32>, word_is_optional: bool) {
        if let Some(group_state) = self.get_last_group_mut() {
            if let Some(dest_handler) = rtf_control::DESTINATIONS.get(name) {
                dest_handler(group_state, name, arg);
            } else if let Some(symbol_handler) = rtf_control::SYMBOLS.get(name) {
                symbol_handler(group_state, name, arg);
            } else if let Some(value_handler) = rtf_control::VALUES.get(name) {
                value_handler(group_state, name, arg);
            } else if let Some(flag_handler) = rtf_control::FLAGS.get(name) {
                flag_handler(group_state, name, arg);
            } else if let Some(toggle_handler) = rtf_control::TOGGLES.get(name) {
                toggle_handler(group_state, name, arg);
            } else if word_is_optional {
                warn!("Skipping optional unsupported control word \\{}", name);
            } else {
                warn!("Unsupported/illegal control word \\{}", name);
            }
        } else {
            warn!(
                "Document format error: Control word found outside of any document group: '\\{}'",
                name
            );
        }
    }

    fn write_to_current_destination(&mut self, bytes: &[u8]) {
        if let Some(group) = self.get_last_group_mut() {
            group.write(bytes);
        } else {
            // it is a fundamental document formatting error for text to appear outside of the {\rtf1 } group
            warn!(
                "Document format error: Document text found outside of any document group: '{:?}'",
                bytes
            );
        }
    }

    fn start_group(&mut self) {
        if let Some(last_group) = self.get_last_group() {
            self.group_stack.push(last_group.clone());
        } else {
            debug!("Creating initial group...");
            self.group_stack
                .push(GroupState::new(self.destinations.clone()));
        }
    }

    fn end_group(&mut self) {
        if let Some(_group) = self.group_stack.pop() {
            // TODO: destination-folding support (tables, etc)
        } else {
            warn!("Document format error: End group count exceeds number start groups");
        }
    }

    fn get_last_group_mut(&mut self) -> Option<&mut GroupState> {
        self.group_stack.last_mut()
    }

    fn get_last_group(&self) -> Option<&GroupState> {
        self.group_stack.last()
    }

    fn process_token(&mut self, token: &Token) {
        let word_is_optional = self
            .get_last_group_mut()
            .map(|group| group.get_and_clear_ignore_next_control())
            .unwrap_or(false);

        // Update state for this token
        match token {
            Token::ControlSymbol(c) => self.do_control_symbol(*c, word_is_optional),
            Token::ControlWord { name, arg } => self.do_control_word(name, *arg, word_is_optional),
            Token::ControlBin(data) => self.do_control_bin(data, word_is_optional),
            Token::Text(bytes) => self.write_to_current_destination(bytes),
            Token::StartGroup => self.start_group(),
            Token::EndGroup => self.end_group(),
            _ => (),
        }
    }
}

pub fn tokenize(data: Vec<u8>) -> Result<Vec<Token>> {
    debug!("Parsing into token stream.");
    parse_tokens(&data).map_err(|e| anyhow::anyhow!("Error parsing RTF tokens: {}", e))
}

pub fn write_plaintext<W: Write>(token_stream: &[Token], mut writer: W) -> Result<()> {
    let mut state = DocumentState::new();

    debug!("Iterating over token stream.");
    for token in token_stream.iter().filter(|c| !matches!(**c, Token::Newline(_))) {
        state.process_token(token);
    }
    debug!("Finished token stream iteration.");

    if let Some(dest) = (*state.destinations).borrow().get("rtf") {
        debug!("Writing rtf1 content...");
        writer
            .write(dest.as_bytes())
            .context("Error writing to output file")?;
    }
    Ok(())
}
