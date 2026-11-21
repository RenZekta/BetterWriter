//! MIDI keyboard input: one open input port whose note messages are queued
//! for the UI thread.

use std::sync::mpsc::{self, Receiver, Sender};

use midir::{Ignore, MidiInput, MidiInputConnection};

const CLIENT_NAME: &str = "BetterWriter";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEvent {
    On { key: u8, velocity: u8 },
    Off { key: u8 },
}

/// A note-on with velocity 0 counts as a note-off, as running-status
/// keyboards send it.
pub fn parse(message: &[u8]) -> Option<KeyEvent> {
    let (&status, data) = message.split_first()?;
    let (&key, &velocity) = (data.first()?, data.get(1).unwrap_or(&0));
    if key > 127 {
        return None;
    }
    match status & 0xF0 {
        0x90 if velocity > 0 => Some(KeyEvent::On { key, velocity }),
        0x90 | 0x80 => Some(KeyEvent::Off { key }),
        _ => None,
    }
}

pub struct MidiKeyboard {
    connection: Option<MidiInputConnection<()>>,
    port_name: Option<String>,
    sender: Sender<KeyEvent>,
    receiver: Receiver<KeyEvent>,
}

impl MidiKeyboard {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            connection: None,
            port_name: None,
            sender,
            receiver,
        }
    }

    /// Names of the input ports present now.
    pub fn ports() -> Vec<String> {
        let Ok(input) = MidiInput::new(CLIENT_NAME) else {
            return Vec::new();
        };
        input
            .ports()
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect()
    }

    pub fn connected(&self) -> Option<&str> {
        self.port_name.as_deref()
    }

    /// Opens the port called `name`; `wake` runs for every received note so
    /// the UI redraws without waiting for other input.
    pub fn connect(&mut self, name: &str, wake: impl Fn() + Send + 'static) -> Result<(), String> {
        self.disconnect();
        let mut input = MidiInput::new(CLIENT_NAME).map_err(|err| err.to_string())?;
        input.ignore(Ignore::All);
        let port = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|found| found == name))
            .ok_or_else(|| format!("{name} is no longer available."))?;
        let sender = self.sender.clone();
        let connection = input
            .connect(
                &port,
                "BetterWriter input",
                move |_, message, _| {
                    if let Some(event) = parse(message)
                        && sender.send(event).is_ok()
                    {
                        wake();
                    }
                },
                (),
            )
            .map_err(|err| err.to_string())?;
        self.connection = Some(connection);
        self.port_name = Some(name.to_owned());
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.connection = None;
        self.port_name = None;
        while self.receiver.try_recv().is_ok() {}
    }

    /// Everything received since the last call, oldest first.
    pub fn drain(&self) -> Vec<KeyEvent> {
        self.receiver.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_on_and_off_messages_are_parsed_on_any_channel() {
        assert_eq!(parse(&[0x90, 60, 100]), Some(KeyEvent::On { key: 60, velocity: 100 }));
        assert_eq!(parse(&[0x95, 40, 1]), Some(KeyEvent::On { key: 40, velocity: 1 }));
        assert_eq!(parse(&[0x80, 60, 64]), Some(KeyEvent::Off { key: 60 }));
        assert_eq!(parse(&[0x8F, 61, 0]), Some(KeyEvent::Off { key: 61 }));
    }

    #[test]
    fn a_note_on_with_no_velocity_releases_the_key() {
        assert_eq!(parse(&[0x90, 60, 0]), Some(KeyEvent::Off { key: 60 }));
    }

    #[test]
    fn other_messages_are_ignored() {
        assert_eq!(parse(&[0xB0, 7, 100]), None);
        assert_eq!(parse(&[0xE0, 0, 64]), None);
        assert_eq!(parse(&[0xF8]), None);
        assert_eq!(parse(&[]), None);
        assert_eq!(parse(&[0x90]), None);
    }
}
